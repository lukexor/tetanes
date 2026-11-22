//! `UN1ROM` (Mapper 094) and `UNROM` with a 74HC08 (Mapper 180).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_094>
//! <https://www.nesdev.org/wiki/INES_Mapper_180>

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

/// Which `UNROM` variant a board is.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    /// Mapper 094: the bank number sits in bits 2-4 of the register.
    Un1rom,
    /// Mapper 180: `$8000` is fixed to the first bank and the register banks `$C000`.
    Hc08,
}

/// `UN1ROM` (Mapper 094) and `UNROM` with a 74HC08 (Mapper 180).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct UnromVariant {
    /// Which variant the board is.
    pub revision: Revision,
    /// The hard-wired nametable arrangement.
    pub mirroring: Mirroring,
    /// The selected 16K PRG bank, at `$8000` on 094 and `$C000` on 180.
    pub prg_bank: u8,
}

impl UnromVariant {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K CHR-RAM
    // CPU $8000..=$BFFF 16K PRG-ROM, switchable on 094 and fixed to the first bank on 180
    // CPU $C000..=$FFFF 16K PRG-ROM, fixed to the last bank on 094 and switchable on 180
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart, revision: Revision) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            revision,
            mirroring: cart.mirroring(),
            prg_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for UnromVariant {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        let name = match self.revision {
            Revision::Un1rom => "PRG $8000",
            Revision::Hc08 => "PRG $C000",
        };
        out.push((name, u32::from(self.prg_bank)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        self.prg_bank = match self.revision {
            // [...P PP..]
            Revision::Un1rom => (val >> 2) & 0x07,
            Revision::Hc08 => val & 0x07,
        };
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        let bank = i32::from(self.prg_bank);
        let (bank_8000, bank_c000) = match self.revision {
            Revision::Un1rom => (bank, -1),
            Revision::Hc08 => (0, bank),
        };
        memory.map_prg(0x8000, Self::PRG_WINDOW, bank_8000, Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, bank_c000, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks) and 8K CHR-RAM.
    fn load(revision: Revision) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 0);
        let mapper = UnromVariant::load(&mut cart, revision).expect("valid mapper");
        (mapper, cart)
    }

    /// Mapper 094 takes the `$8000` bank from bits 2-4, under a fixed last bank.
    #[test]
    fn un1rom_banks_8000_from_bits_2_to_4() {
        let (mut mapper, mut cart) = load(Revision::Un1rom);
        write(&mut mapper, &mut cart, 0x8000, 0x14);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 5 * 16, "PRG 5");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "fixed last");
    }

    /// Mapper 180 fixes `$8000` to the first bank and banks `$C000`.
    #[test]
    fn hc08_banks_c000() {
        let (mut mapper, mut cart) = load(Revision::Hc08);
        write(&mut mapper, &mut cart, 0x8000, 0x06);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "fixed first");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 6 * 16, "PRG 6");
    }
}
