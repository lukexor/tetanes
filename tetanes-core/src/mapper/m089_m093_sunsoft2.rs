//! `Sunsoft-2` (Mappers 089, 093).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_089>
//! <https://www.nesdev.org/wiki/INES_Mapper_093>

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

/// `Sunsoft-2` board wiring.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    /// Mapper 089: switchable CHR-ROM and one-screen mirroring.
    ChrRom,
    /// Mapper 093: CHR-RAM behind an enable bit, and hard-wired mirroring.
    ChrRam,
}

/// `Sunsoft-2` (Mappers 089, 093).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sunsoft2 {
    /// Which board the chip sits on.
    pub revision: Revision,
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// The selected 16K PRG bank at `$8000`.
    pub prg_bank: u8,
    /// The selected 8K CHR bank. Always 0 on mapper 093.
    pub chr_bank: u8,
    /// Mapper 093 only: whether CHR-RAM responds to the PPU.
    pub chr_enabled: bool,
}

impl Sunsoft2 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K CHR, switchable ROM on 089 and RAM with an enable on 093
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart, revision: Revision) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            revision,
            mirroring: cart.mirroring(),
            prg_bank: 0,
            chr_bank: 0,
            chr_enabled: true,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Sunsoft2 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("PRG $8000", u32::from(self.prg_bank)));
        match self.revision {
            Revision::ChrRom => out.push(("CHR $0000", u32::from(self.chr_bank))),
            Revision::ChrRam => out.push(("CHR enabled", u32::from(self.chr_enabled))),
        }
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        self.prg_bank = (val >> 4) & 0x07;
        match self.revision {
            Revision::ChrRom => {
                // Bits 0-2 and 7 are the CHR bank, with bit 7 as its high bit. Bit 3 is mirroring.
                self.chr_bank = ((val & 0x80) >> 4) | (val & 0x07);
                self.mirroring = if val & 0x08 == 0x08 {
                    Mirroring::SingleScreenB
                } else {
                    Mirroring::SingleScreenA
                };
            }
            // [.PPP ...E]
            Revision::ChrRam => self.chr_enabled = val & 0x01 == 0x01,
        }
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        if self.chr_enabled {
            memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
        } else {
            memory.unmap_chr(0x0000, Self::CHR_WINDOW);
        }
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks), and 128K CHR-ROM (16 8K banks) or 8K CHR-RAM.
    fn load(revision: Revision) -> (Mapper, Cart) {
        let chr = match revision {
            Revision::ChrRom => 128 * 1024,
            Revision::ChrRam => 0,
        };
        let mut cart = page_indexed_cart(128 * 1024, 0, chr);
        let mapper = Sunsoft2::load(&mut cart, revision).expect("valid mapper");
        (mapper, cart)
    }

    /// Mapper 089 takes PRG from bits 4-6, CHR from bits 0-2 plus bit 7, and one-screen mirroring
    /// from bit 3.
    #[test]
    fn mapper_089_register() {
        let (mut mapper, mut cart) = load(Revision::ChrRom);
        write(&mut mapper, &mut cart, 0x8000, 0xB5);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "PRG 3");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "fixed last");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (13 * 8), "CHR 13");
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenA);
        write(&mut mapper, &mut cart, 0x8000, 0x08);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB);
    }

    /// Mapper 093 takes PRG from bits 4-6, and bit 0 connects CHR-RAM.
    #[test]
    fn mapper_093_register() {
        let (mut mapper, mut cart) = load(Revision::ChrRam);
        let mirroring = mapper.mirroring();
        write(&mut mapper, &mut cart, 0x8000, 0x21);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 16, "PRG 2");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80, "CHR-RAM enabled");
        write(&mut mapper, &mut cart, 0x8000, 0x20);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x00, "CHR-RAM disabled");
        assert_eq!(mapper.mirroring(), mirroring, "hard-wired");
    }
}
