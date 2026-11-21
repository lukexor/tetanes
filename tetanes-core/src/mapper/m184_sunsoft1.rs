//! `Sunsoft-1` (Mapper 184).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_184>

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

/// `Sunsoft-1` (Mapper 184).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sunsoft1 {
    /// The hard-wired nametable arrangement.
    pub mirroring: Mirroring,
    /// The bank register: the $0000 CHR bank in bits 0-2, the $1000 bank in bits 4-6.
    pub reg: u8,
}

impl Sunsoft1 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;

    // PPU $0000..=$0FFF 4K switchable CHR-ROM bank
    // PPU $1000..=$1FFF 4K switchable CHR-ROM bank, from the upper half of CHR-ROM
    // CPU $6000..=$7FFF CHR bank register, with no PRG-RAM
    // CPU $8000..=$FFFF 16K or 32K fixed PRG-ROM
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            reg: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    const fn chr_lo(&self) -> u8 {
        self.reg & 0x07
    }

    // The board ties the top bit of the $1000 bank high.
    const fn chr_hi(&self) -> u8 {
        ((self.reg >> 4) & 0x07) | 0x04
    }
}

impl Map for Sunsoft1 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("CHR $0000", u32::from(self.chr_lo())));
        out.push(("CHR $1000", u32::from(self.chr_hi())));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.reg = val;
            self.update_banks(memory);
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        // A 16K cart maps the same bank into both slots, which falls out of the bank index
        // wrapping within the region.
        memory.map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(
            0x0000,
            Self::CHR_WINDOW,
            i32::from(self.chr_lo()),
            Src::Chr,
        );
        memory.map_chr(
            0x1000,
            Self::CHR_WINDOW,
            i32::from(self.chr_hi()),
            Src::Chr,
        );
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, write};

    /// Each nibble banks one 4K half, and the $1000 half always comes from banks 4-7.
    #[test]
    fn nibbles_bank_each_half_with_the_high_half_in_banks_4_to_7() {
        let mut cart = page_indexed_cart(32 * 1024, 0, 32 * 1024);
        let mut mapper = Sunsoft1::load(&mut cart).expect("valid mapper");
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | (4 * 4), "bank 4 at power-on");
        write(&mut mapper, &mut cart, 0x6000, 0x12);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (2 * 4));
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | (5 * 4));
    }
}
