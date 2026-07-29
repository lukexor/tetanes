//! `CNROM (Mapper 003)`.
//!
//! <https://wiki.nesdev.org/w/index.php/CNROM>

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

/// `CNROM` (Mapper 003).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Cnrom {
    pub mirroring: Mirroring,
    pub chr_bank: u8,
}

impl Cnrom {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
    // CPU $8000..=$FFFF 16K or 32K Fixed PRG-ROM
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        // A 16K cart maps the same bank into both slots, which falls out of the bank index
        // wrapping within the region.
        let mut board = Self {
            mirroring: cart.mirroring(),
            chr_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Cnrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("CHR", u32::from(self.chr_bank)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            self.chr_bank = val;
            memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(val), Src::Chr);
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        // A 16K cart maps the same bank into both slots, which falls out of the bank index
        // wrapping within the region.
        memory.map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// `prg_rom` bytes of PRG-ROM, no PRG-RAM, 32K CHR-ROM (4 8K banks).
    fn load(prg_rom: usize) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(prg_rom, 0, 32 * 1024);
        let mapper = Cnrom::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    fn cnrom() -> (Mapper, Cart) {
        load(32 * 1024)
    }

    /// CNROM banks CHR only; both PRG halves are fixed for the life of the cart.
    #[test]
    fn prg_is_fixed_and_chr_starts_at_bank_0() {
        let (mapper, cart) = cnrom();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80);
    }

    /// A 16K cart maps its one bank into both slots, out of the bank index wrapping within the
    /// region rather than a mirror flag.
    #[test]
    fn a_16k_cart_mirrors_its_single_prg_bank() {
        let (mapper, cart) = load(16 * 1024);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 15, "mirrored reset vectors");
    }

    /// Any write to $8000-$FFFF selects the 8K CHR bank, and PRG never moves with it.
    #[test]
    fn a_write_anywhere_above_8000_selects_the_chr_bank() {
        let (mut mapper, mut cart) = cnrom();
        for (addr, bank) in [(0x8000u16, 1u8), (0xBFFF, 2), (0xFFFF, 3)] {
            write(&mut mapper, &mut cart, addr, bank);
            assert_eq!(
                chr_peek(&mapper, &cart, 0x0000),
                0x80 | (bank * 8),
                "${addr:04X} selects CHR bank {bank}"
            );
            assert_eq!(chr_peek(&mapper, &cart, 0x1FFF), 0x80 | (bank * 8 + 7));
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "PRG is unaffected");
        }
    }

    /// The board decodes only $8000-$FFFF; below that is the bus, not a register.
    #[test]
    fn writes_below_8000_are_ignored() {
        let (mut mapper, mut cart) = cnrom();
        write(&mut mapper, &mut cart, 0x8000, 2);
        for addr in [0x4100, 0x6000, 0x7FFF] {
            write(&mut mapper, &mut cart, addr, 1);
            assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x90, "${addr:04X}");
        }
    }

    /// The register is a full byte and CNROM has at most 4 banks, so a high bank wraps within the
    /// region rather than reading outside it.
    #[test]
    fn a_bank_past_the_end_of_the_chr_rom_wraps() {
        let (mut mapper, mut cart) = cnrom();
        write(&mut mapper, &mut cart, 0x8000, 0xFF);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (3 * 8), "wraps to 3");
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = cnrom();
        write(&mut mapper, &mut cart, 0x8000, 2);

        let sample = |mapper: &Mapper, cart: &Cart| {
            [
                prg_peek(mapper, cart, 0x8000),
                prg_peek(mapper, cart, 0xC000),
                chr_peek(mapper, cart, 0x0000),
            ]
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
    }
}
