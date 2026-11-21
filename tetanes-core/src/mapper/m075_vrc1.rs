//! `VRC1` (Mapper 075).
//!
//! <https://www.nesdev.org/wiki/VRC1>

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

/// `VRC1` (Mapper 075).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc1 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// `$8000`, `$A000`, `$C000`: 8K PRG banks.
    pub prg: [u8; 3],
    /// The two 4K CHR banks, with the high bit from `$9000` already merged in.
    pub chr: [u8; 2],
}

impl Vrc1 {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;

    // PPU $0000..=$0FFF 4K switchable CHR-ROM bank
    // PPU $1000..=$1FFF 4K switchable CHR-ROM bank
    // CPU $8000..=$DFFF three 8K switchable PRG-ROM banks
    // CPU $E000..=$FFFF 8K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg: [0; 3],
            chr: [0; 2],
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Vrc1 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        for (name, reg) in ["PRG $8000", "PRG $A000", "PRG $C000"]
            .into_iter()
            .zip(self.prg)
        {
            out.push((name, u32::from(reg)));
        }
        out.push(("CHR $0000", u32::from(self.chr[0])));
        out.push(("CHR $1000", u32::from(self.chr[1])));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr & 0xF000 {
            0x8000 => self.prg[0] = val & 0x0F,
            0xA000 => self.prg[1] = val & 0x0F,
            0xC000 => self.prg[2] = val & 0x0F,
            0x9000 => {
                // A four-screen cart has its own nametable RAM and ignores the mirroring bit.
                if self.mirroring != Mirroring::FourScreen {
                    self.mirroring = if val & 0x01 == 0x01 {
                        Mirroring::Horizontal
                    } else {
                        Mirroring::Vertical
                    };
                }
                self.chr[0] = (self.chr[0] & 0x0F) | ((val & 0x02) << 3);
                self.chr[1] = (self.chr[1] & 0x0F) | ((val & 0x04) << 2);
            }
            0xE000 => self.chr[0] = (self.chr[0] & 0x10) | (val & 0x0F),
            0xF000 => self.chr[1] = (self.chr[1] & 0x10) | (val & 0x0F),
            _ => return,
        }
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        for (slot, reg) in self.prg.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, i32::from(*reg), Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr[0]), Src::Chr);
        memory.map_chr(0x1000, Self::CHR_WINDOW, i32::from(self.chr[1]), Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 64K PRG-ROM (8 8K banks) and 128K CHR-ROM (32 4K banks).
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(64 * 1024, 0, 128 * 1024);
        let mapper = Vrc1::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Three switchable PRG banks and a fixed last bank.
    #[test]
    fn three_prg_registers_and_a_fixed_last_bank() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x8000, 2);
        write(&mut mapper, &mut cart, 0xA000, 3);
        write(&mut mapper, &mut cart, 0xC000, 4);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 4 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 7 * 8);
    }

    /// `$9000` supplies bit 4 of each CHR bank, and the low nibbles keep it when written later.
    #[test]
    fn chr_high_bits_come_from_9000() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x9000, 0x04);
        write(&mut mapper, &mut cart, 0xE000, 0x03);
        write(&mut mapper, &mut cart, 0xF000, 0x05);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (3 * 4));
        assert_eq!(
            chr_peek(&mapper, &cart, 0x1000),
            0x80 | (21 * 4),
            "bank $15"
        );
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    }
}
