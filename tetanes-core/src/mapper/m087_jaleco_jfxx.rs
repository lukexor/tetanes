//! `Jaleco JF-xx` (Mapper 087).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_087>

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

/// `Jaleco JF-xx` (Mapper 087).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct JalecoJfxx {
    /// The hard-wired nametable arrangement.
    pub mirroring: Mirroring,
    /// The selected 8K CHR bank.
    pub chr_bank: u8,
}

impl JalecoJfxx {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K switchable CHR-ROM bank
    // CPU $6000..=$7FFF CHR bank register, with no PRG-RAM
    // CPU $8000..=$FFFF 16K or 32K fixed PRG-ROM
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            chr_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for JalecoJfxx {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("CHR $0000", u32::from(self.chr_bank)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            // The board wires D0 to CHR A14 and D1 to A13, so the two bits arrive swapped.
            self.chr_bank = ((val & 0x01) << 1) | ((val >> 1) & 0x01);
            memory.map_chr(
                0x0000,
                Self::CHR_WINDOW,
                i32::from(self.chr_bank),
                Src::Chr,
            );
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
            i32::from(self.chr_bank),
            Src::Chr,
        );
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, write};

    /// The register sits at $6000-$7FFF with its two bits swapped, and $8000 is not a register.
    #[test]
    fn chr_register_at_6000_has_swapped_bits() {
        let mut cart = page_indexed_cart(32 * 1024, 0, 32 * 1024);
        let mut mapper = JalecoJfxx::load(&mut cart).expect("valid mapper");
        for (val, bank) in [(0x01, 2), (0x02, 1), (0x03, 3), (0x00, 0)] {
            write(&mut mapper, &mut cart, 0x6000, val);
            assert_eq!(
                chr_peek(&mapper, &cart, 0x0000),
                0x80 | (bank * 8),
                "${val:02X} selects bank {bank}"
            );
        }
        write(&mut mapper, &mut cart, 0x8000, 0x03);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80, "$8000 is ROM");
    }
}
