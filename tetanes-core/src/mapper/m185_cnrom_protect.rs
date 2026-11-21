//! `CNROM` with CHR copy protection (Mapper 185).
//!
//! <https://www.nesdev.org/wiki/CNROM>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `CNROM` with CHR copy protection (Mapper 185).
///
/// The game writes a value that disables CHR, checks that the pattern tables read back as
/// garbage, and locks up if they don't.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct CnromProtect {
    /// The hard-wired nametable arrangement.
    pub mirroring: Mirroring,
    /// NES 2.0 submapper. 4-7 name the value that enables CHR.
    pub submapper_num: u8,
    /// The last value latched, after bus conflicts.
    pub reg: u8,
    /// CHR reads since reset, counted up to the two that submapper 0 answers as open bus.
    pub reads: u8,
}

impl CnromProtect {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    /// Reads after reset that submapper 0 answers as disabled CHR.
    const DISABLED_READS: u8 = 2;

    // PPU $0000..=$1FFF 8K CHR-ROM, or open bus while disabled
    // CPU $8000..=$FFFF 16K or 32K fixed PRG-ROM, and the register with bus conflicts
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            submapper_num: cart.submapper_num(),
            reg: 0,
            reads: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    /// Whether CHR-ROM answers reads.
    //
    // Without a submapper the enabling value is unknown. Every known game checks for garbage with
    // its first two reads after reset and never again, so those two reads are disabled and the
    // rest are not.
    const fn chr_enabled(&self) -> bool {
        match self.submapper_num {
            4..=7 => self.reg & 0x03 == self.submapper_num - 4,
            _ => self.reads >= Self::DISABLED_READS,
        }
    }

    /// What a disabled CHR read returns.
    //
    // Open bus, which the PPU leaves holding the low address byte. Mighty Bomb Jack's board pulls
    // D0 high and the game checks $0000 for anything but $00.
    const fn open_bus(addr: u16) -> u8 {
        (addr as u8) | 0x01
    }
}

impl Map for CnromProtect {
    // Disabled CHR reads are served here, since no page entry yields a byte that depends on the
    // address.
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::SERVES_CHR_READS
    }

    fn chr_read(&mut self, memory: &mut Memory, addr: u16) -> Option<u8> {
        let val = self.chr_peek(memory, addr);
        if addr < 0x2000 && self.reads < Self::DISABLED_READS {
            self.reads += 1;
        }
        val
    }

    fn chr_peek(&self, _memory: &Memory, addr: u16) -> Option<u8> {
        (addr < 0x2000 && !self.chr_enabled()).then_some(Self::open_bus(addr))
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("Latch", u32::from(self.reg)));
        out.push(("CHR enabled", u32::from(self.chr_enabled())));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            self.reg = val & memory.prg_peek(addr);
        }
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.reads = 0;
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        // A 16K cart maps the same bank into both slots, which falls out of the bank index
        // wrapping within the region.
        memory.map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, write};

    /// 32K PRG-ROM of `$FF`, so bus conflicts pass every written bit, and 8K CHR-ROM.
    fn load(submapper_num: u8) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(32 * 1024, 0, 8 * 1024);
        cart.memory.region_mut(Src::PrgRom).fill(0xFF);
        cart.header.submapper_num = submapper_num;
        let mapper = CnromProtect::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    fn chr_read(mapper: &mut Mapper, cart: &mut Cart, addr: u16) -> u8 {
        mapper
            .chr_read(&mut cart.memory, addr)
            .unwrap_or_else(|| cart.memory.chr_peek(addr))
    }

    /// Without a submapper, the first two reads after reset are open bus and the rest are CHR.
    #[test]
    fn submapper_0_disables_the_first_two_reads_after_reset() {
        let (mut mapper, mut cart) = load(0);
        for _ in 0..2 {
            assert_eq!(chr_read(&mut mapper, &mut cart, 0x0410), 0x11, "open bus");
        }
        assert_eq!(chr_read(&mut mapper, &mut cart, 0x0410), 0x81, "CHR page 1");

        mapper.reset(ResetKind::Soft);
        assert_eq!(
            chr_read(&mut mapper, &mut cart, 0x0000),
            0x01,
            "D0 pulled high"
        );
    }

    /// Submappers 4-7 enable CHR for one value of the low two bits each.
    #[test]
    fn submapper_names_the_enabling_value() {
        let (mut mapper, mut cart) = load(6);
        write(&mut mapper, &mut cart, 0x8000, 0x21);
        assert_eq!(chr_peek(&mapper, &cart, 0x0020), 0x21, "open bus");
        write(&mut mapper, &mut cart, 0x8000, 0x22);
        assert_eq!(chr_peek(&mapper, &cart, 0x0020), 0x80);
    }

    /// The latch receives the written value AND the ROM byte under it.
    #[test]
    fn writes_have_bus_conflicts() {
        let (mut mapper, mut cart) = load(7);
        cart.memory.region_mut(Src::PrgRom)[0] = 0x01;
        write(&mut mapper, &mut cart, 0x8000, 0x03);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x01, "$03 & $01 disables");
    }
}
