//! `GxROM (Mapper 066)`.
//!
//! <https://wiki.nesdev.org/w/index.php/GxROM>

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

/// `GxROM` (Mapper 066).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Gxrom {
    pub mirroring: Mirroring,
    pub chr_bank: u8,
    pub prg_bank: u8,
}

impl Gxrom {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    const CHR_BANK_MASK: u8 = 0x0F;
    const PRG_BANK_MASK: u8 = 0x30;

    // PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            chr_bank: 0,
            prg_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Gxrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            self.chr_bank = val & Self::CHR_BANK_MASK;
            self.prg_bank = (val & Self::PRG_BANK_MASK) >> 4;
            memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
            memory.map_prg(
                0x8000,
                Self::PRG_WINDOW,
                i32::from(self.prg_bank),
                Src::PrgRom,
            );
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (4 32K banks), no PRG-RAM, 64K CHR-ROM (8 8K banks).
    fn gxrom() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 64 * 1024);
        let mapper = Gxrom::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// One byte holds both banks: PRG in bits 5-4, CHR in the low bits.
    #[test]
    fn one_write_sets_both_banks_from_opposite_ends_of_the_byte() {
        let (mut mapper, mut cart) = gxrom();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "bank 0 at power-on");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80);

        write(&mut mapper, &mut cart, 0x8000, 0x23);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 32, "PRG bank 2");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 2 * 32 + 16, "one 32K bank");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (3 * 8), "CHR bank 3");
    }

    /// Bits 7-6 belong to neither register, and PRG is only the two bits above the CHR nibble.
    #[test]
    fn the_top_two_bits_are_not_part_of_either_bank() {
        let (mut mapper, mut cart) = gxrom();
        write(&mut mapper, &mut cart, 0x8000, 0xC0);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "PRG bank 0");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80, "CHR bank 0");

        // The CHR field is decoded as a whole nibble, where the board itself only wires up the two
        // bits its 32K of CHR-ROM needs. The extra bits can only ever select a bank the cart does
        // not have, which wraps within the region.
        write(&mut mapper, &mut cart, 0x8000, 0x0F);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (7 * 8), "wraps");
    }

    /// The board decodes only $8000-$FFFF; below that is the bus, not a register.
    #[test]
    fn writes_below_8000_are_ignored() {
        let (mut mapper, mut cart) = gxrom();
        write(&mut mapper, &mut cart, 0x8000, 0x23);
        for addr in [0x4100, 0x6000, 0x7FFF] {
            write(&mut mapper, &mut cart, addr, 0x00);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 32, "${addr:04X}");
        }
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = gxrom();
        write(&mut mapper, &mut cart, 0x8000, 0x23);

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
