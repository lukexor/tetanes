//! `Jaleco JF-13` (Mapper 086).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_086>

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

/// `Jaleco JF-13` (Mapper 086).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct JalecoJf13 {
    /// The hard-wired nametable arrangement.
    pub mirroring: Mirroring,
    /// The selected 32K PRG bank.
    pub prg_bank: u8,
    /// The selected 8K CHR bank.
    pub chr_bank: u8,
}

impl JalecoJf13 {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K switchable CHR-ROM bank
    // CPU $6000..=$6FFF PRG/CHR bank register, with no PRG-RAM
    // CPU $7000..=$7FFF uPD7756 speech chip control
    // CPU $8000..=$FFFF 32K switchable PRG-ROM bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg_bank: 0,
            chr_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for JalecoJf13 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("PRG $8000", u32::from(self.prg_bank)));
        out.push(("CHR $0000", u32::from(self.chr_bank)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        // $7000-$7FFF drives the speech chip, whose samples live in its own mask ROM rather than
        // the dump, so there is nothing to play and the writes are dropped.
        if (0x6000..=0x6FFF).contains(&addr) {
            // [.CPP ..CC]: bit 6 is the high CHR bit, above the two low ones.
            self.prg_bank = (val >> 4) & 0x03;
            self.chr_bank = ((val >> 4) & 0x04) | (val & 0x03);
            self.update_banks(memory);
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

    /// $6000 selects PRG from bits 4-5 and CHR from bits 0-1 plus bit 6. $7000 and $8000 are not
    /// bank registers.
    #[test]
    fn bank_register_at_6000() {
        let mut cart = page_indexed_cart(128 * 1024, 0, 64 * 1024);
        let mut mapper = JalecoJf13::load(&mut cart).expect("valid mapper");
        for (val, prg, chr) in [
            (0x00, 0, 0),
            (0x10, 1, 0),
            (0x30, 3, 0),
            (0x03, 0, 3),
            (0x40, 0, 4),
            (0x73, 3, 7),
        ] {
            write(&mut mapper, &mut cart, 0x6000, val);
            assert_eq!(
                prg_peek(&mapper, &cart, 0x8000),
                prg * 32,
                "${val:02X} selects PRG bank {prg}"
            );
            assert_eq!(
                chr_peek(&mapper, &cart, 0x0000),
                0x80 | (chr * 8),
                "${val:02X} selects CHR bank {chr}"
            );
        }
        write(&mut mapper, &mut cart, 0x7000, 0x00);
        write(&mut mapper, &mut cart, 0x8000, 0x00);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x8000),
            96,
            "$7000/$8000 leave PRG"
        );
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0000),
            0x80 | 56,
            "$7000/$8000 leave CHR"
        );
    }
}
