//! `Bandai 74161/7432` (Mappers 070, 152).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_070>
//! <https://www.nesdev.org/wiki/INES_Mapper_152>

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

/// `Bandai 74161/7432` (Mappers 070, 152).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bandai74161 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// Mapper 152 selects one-screen mirroring with bit 7 of the register.
    pub mapper_num: u16,
    /// The bank register: PRG bank in the high nibble, CHR bank in the low.
    pub reg: u8,
}

impl Bandai74161 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    /// Mapper 152's one-screen select.
    const SINGLE_SCREEN_B: u8 = 0x80;

    // PPU $0000..=$1FFF 8K switchable CHR-ROM bank
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mapper_num = cart.mapper_num();
        let mut board = Self {
            mirroring: if mapper_num == 152 {
                Mirroring::SingleScreenA
            } else {
                cart.mirroring()
            },
            mapper_num,
            reg: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    const fn prg_bank(&self) -> u8 {
        if self.mapper_num == 152 {
            (self.reg >> 4) & 0x07
        } else {
            self.reg >> 4
        }
    }
}

impl Map for Bandai74161 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("PRG $8000", u32::from(self.prg_bank())));
        out.push(("CHR $0000", u32::from(self.reg & 0x0F)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            self.reg = val;
            if self.mapper_num == 152 {
                self.mirroring = if val & Self::SINGLE_SCREEN_B == Self::SINGLE_SCREEN_B {
                    Mirroring::SingleScreenB
                } else {
                    Mirroring::SingleScreenA
                };
            }
            self.update_banks(memory);
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank()),
            Src::PrgRom,
        );
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(
            0x0000,
            Self::CHR_WINDOW,
            i32::from(self.reg & 0x0F),
            Src::Chr,
        );
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks) and 64K CHR-ROM (8 8K banks), loaded as `mapper_num`.
    fn load(mapper_num: u16) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 64 * 1024);
        cart.header.mapper_num = mapper_num;
        let mapper = Bandai74161::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// The high nibble banks PRG at $8000 and the low nibble banks CHR.
    #[test]
    fn one_write_banks_prg_and_chr() {
        let (mut mapper, mut cart) = load(70);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "last bank fixed");
        write(&mut mapper, &mut cart, 0x8000, 0x35);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (5 * 8));
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "still fixed");
    }

    /// Mapper 152 takes bit 7 for one-screen mirroring, leaving three PRG bits.
    #[test]
    fn mapper_152_selects_one_screen_with_bit_7() {
        let (mut mapper, mut cart) = load(152);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenA);
        write(&mut mapper, &mut cart, 0x8000, 0xA2);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 16);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (2 * 8));
    }
}
