//! `Sunsoft-4` (Mapper 068).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_068>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `Sunsoft-4` (Mapper 068).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sunsoft4 {
    /// `$E000` bits 0-1: the nametable arrangement.
    pub mirroring: Mirroring,
    /// `$E000` bit 4: nametables come from CHR-ROM instead of CIRAM.
    pub chr_nametables: bool,
    /// `$8000-$B000`: four 2K CHR banks.
    pub chr: [u8; 4],
    /// `$C000` and `$D000`: the 1K CHR-ROM banks used as nametables, from the last 128K of CHR.
    pub nametables: [u8; 2],
    /// `$F000` bits 0-3: the 16K PRG bank at `$8000`.
    pub prg: u8,
    /// `$F000` bit 4: PRG-RAM answers at `$6000`.
    pub prg_ram_enabled: bool,
}

impl Sunsoft4 {
    const PRG_WINDOW: usize = 16 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 2 * 1024;
    const NAMETABLE_WINDOW: usize = 1024;
    /// The nametable registers' bank bit 7 is wired high, so they index the last 128K of CHR.
    const NAMETABLE_BANK_BASE: u8 = 0x80;

    // PPU $0000..=$1FFF four 2K switchable CHR-ROM banks
    // PPU $2000..=$2FFF nametables from CIRAM, or two 1K switchable CHR-ROM banks
    // CPU $6000..=$7FFF 8K PRG-RAM, when enabled
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            chr_nametables: false,
            chr: [0; 4],
            nametables: [0; 2],
            prg: 0,
            prg_ram_enabled: false,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    fn update_nametables(&self, memory: &mut Memory) {
        if !self.chr_nametables {
            memory.set_mirroring(self.mirroring);
            return;
        }
        let [a, b] = self.nametables;
        let regs = match self.mirroring {
            Mirroring::Horizontal => [a, a, b, b],
            Mirroring::SingleScreenA => [a, a, a, a],
            Mirroring::SingleScreenB => [b, b, b, b],
            _ => [a, b, a, b],
        };
        for (nametable, reg) in regs.into_iter().enumerate() {
            let addr = 0x2000 + (nametable * Self::NAMETABLE_WINDOW) as u16;
            let bank = i32::from(reg | Self::NAMETABLE_BANK_BASE);
            memory.map_chr(addr, Self::NAMETABLE_WINDOW, bank, Src::Chr);
            // $3000-$3EFF mirrors $2000-$2EFF.
            memory.map_chr(addr + 0x1000, Self::NAMETABLE_WINDOW, bank, Src::Chr);
        }
    }
}

impl Map for Sunsoft4 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        for (name, reg) in ["CHR $0000", "CHR $0800", "CHR $1000", "CHR $1800"]
            .into_iter()
            .zip(self.chr)
        {
            out.push((name, u32::from(reg)));
        }
        out.push(("NT $C000", u32::from(self.nametables[0])));
        out.push(("NT $D000", u32::from(self.nametables[1])));
        out.push(("CHR nametables", u32::from(self.chr_nametables)));
        out.push(("PRG $8000", u32::from(self.prg)));
        out.push(("PRG-RAM enabled", u32::from(self.prg_ram_enabled)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr & 0xF000 {
            reg @ 0x8000..=0xB000 => {
                let slot = usize::from((reg - 0x8000) >> 12);
                self.chr[slot] = val;
                let addr = (slot * Self::CHR_WINDOW) as u16;
                memory.map_chr(addr, Self::CHR_WINDOW, i32::from(val), Src::Chr);
            }
            0xC000 => {
                self.nametables[0] = val & 0x7F;
                self.update_nametables(memory);
            }
            0xD000 => {
                self.nametables[1] = val & 0x7F;
                self.update_nametables(memory);
            }
            0xE000 => {
                self.mirroring = match val & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenA,
                    _ => Mirroring::SingleScreenB,
                };
                self.chr_nametables = val & 0x10 == 0x10;
                self.update_nametables(memory);
            }
            0xF000 => {
                self.prg = val & 0x0F;
                self.prg_ram_enabled = val & 0x10 == 0x10;
                self.update_banks(memory);
            }
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        if self.prg_ram_enabled {
            memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        } else {
            memory.unmap_prg(0x6000, Self::PRG_RAM_WINDOW);
        }
        memory.map_prg(0x8000, Self::PRG_WINDOW, i32::from(self.prg), Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        for (slot, reg) in self.chr.iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*reg), Src::Chr);
        }
        self.update_nametables(memory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks), 8K PRG-RAM and 256K CHR-ROM.
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 256 * 1024);
        let mapper = Sunsoft4::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    fn nametables(mapper: &Mapper, cart: &Cart) -> [u8; 4] {
        [0x2000, 0x2400, 0x2800, 0x2C00].map(|addr| chr_peek(mapper, cart, addr))
    }

    /// One switchable 16K bank, the last fixed, and PRG-RAM only while `$F000` bit 4 is set.
    #[test]
    fn prg_bank_and_ram_enable() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xF000, 0x03);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16);

        write(&mut mapper, &mut cart, 0x6000, 0x42);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x00, "disabled");
        write(&mut mapper, &mut cart, 0xF000, 0x13);
        write(&mut mapper, &mut cart, 0x6000, 0x42);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x42, "enabled");
    }

    /// Four 2K CHR banks, one per register.
    #[test]
    fn chr_banks_are_2k() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xB000, 5);
        assert_eq!(chr_peek(&mapper, &cart, 0x1800), 0x80 | 10);
        assert_eq!(chr_peek(&mapper, &cart, 0x1C00), 0x80 | 11);
    }

    /// With `$E000` bit 4 set, `$C000`/`$D000` pick CHR-ROM pages from the last 128K as
    /// nametables, arranged by the mirroring bits, and writes to them are dropped.
    #[test]
    fn chr_rom_nametables() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xC000, 0x01);
        write(&mut mapper, &mut cart, 0xD000, 0x02);
        // CHR page n reads as `0x80 | n`, and bit 7 of the bank is forced, so these are $81, $82.
        let (a, b) = (0x81, 0x82);

        write(&mut mapper, &mut cart, 0xE000, 0x10);
        assert_eq!(nametables(&mapper, &cart), [a, b, a, b], "vertical");
        write(&mut mapper, &mut cart, 0xE000, 0x11);
        assert_eq!(nametables(&mapper, &cart), [a, a, b, b], "horizontal");
        write(&mut mapper, &mut cart, 0xE000, 0x13);
        assert_eq!(nametables(&mapper, &cart), [b, b, b, b], "one-screen B");
        assert_eq!(chr_peek(&mapper, &cart, 0x3000), b, "$3000 mirrors $2000");

        cart.memory.chr_write(0x2000, 0x55);
        assert_eq!(chr_peek(&mapper, &cart, 0x2000), b, "CHR-ROM is read-only");

        write(&mut mapper, &mut cart, 0xE000, 0x00);
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
        assert_ne!(chr_peek(&mapper, &cart, 0x2000), b, "back to CIRAM");
    }
}
