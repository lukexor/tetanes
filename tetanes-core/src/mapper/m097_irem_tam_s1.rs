//! `Irem TAM-S1` (Mapper 097).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_097>

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

/// `Irem TAM-S1` (Mapper 097).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct IremTamS1 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// The selected 16K PRG bank at `$C000`.
    pub prg_bank: u8,
}

impl IremTamS1 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K CHR-RAM
    // CPU $8000..=$BFFF 16K PRG-ROM fixed to the last bank
    // CPU $C000..=$FFFF 16K switchable PRG-ROM bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            // $C000 must start on the last bank, where the CPU reads its reset vector. The index
            // wraps to the last bank of any PRG size.
            prg_bank: 0x0F,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for IremTamS1 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("PRG $C000", u32::from(self.prg_bank)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        // [MM.. PPPP]
        self.prg_bank = val & 0x0F;
        self.mirroring = match val >> 6 {
            0 => Mirroring::SingleScreenA,
            1 => Mirroring::Horizontal,
            2 => Mirroring::Vertical,
            _ => Mirroring::SingleScreenB,
        };
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x8000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_prg(
            0xC000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{page_indexed_cart, prg_peek, write};

    /// `$8000` is fixed to the last bank, and the register banks `$C000` and sets mirroring.
    #[test]
    fn register_banks_c000_and_sets_mirroring() {
        let mut cart = page_indexed_cart(128 * 1024, 0, 0);
        let mut mapper = IremTamS1::load(&mut cart).expect("valid mapper");
        assert_eq!(
            prg_peek(&mapper, &cart, 0xC000),
            7 * 16,
            "powers on as the last bank"
        );
        for (val, bank, mirroring) in [
            (0x02, 2, Mirroring::SingleScreenA),
            (0x43, 3, Mirroring::Horizontal),
            (0x84, 4, Mirroring::Vertical),
            (0xC5, 5, Mirroring::SingleScreenB),
        ] {
            write(&mut mapper, &mut cart, 0x8000, val);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 7 * 16, "fixed last");
            assert_eq!(prg_peek(&mapper, &cart, 0xC000), bank * 16, "${val:02X}");
            assert_eq!(mapper.mirroring(), mirroring, "${val:02X}");
        }
    }
}
