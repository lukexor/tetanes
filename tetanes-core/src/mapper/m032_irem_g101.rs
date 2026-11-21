//! `Irem G-101` (Mapper 032).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_032>

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

/// `Irem G-101` (Mapper 032).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct IremG101 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// Submapper 1 (Major League) has one-screen mirroring wired in and no `$9000` register.
    pub submapper_num: u8,
    /// `$9000` bit 1: the `$8000` register banks `$C000` instead, and `$8000` is fixed.
    pub prg_swapped: bool,
    /// `$8000` and `$A000`: 8K PRG banks.
    pub prg: [u8; 2],
    /// `$B000-$B007`: 1K CHR banks.
    pub chr: [u8; 8],
}

impl IremG101 {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;

    // PPU $0000..=$1FFF eight 1K switchable CHR-ROM banks
    // CPU $6000..=$7FFF 8K PRG-RAM
    // CPU $8000..=$BFFF two 8K switchable PRG-ROM banks, with $8000 and $C000 swappable
    // CPU $C000..=$FFFF 8K PRG-ROM fixed to the second-last bank, and 8K fixed to the last
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let submapper_num = cart.submapper_num();
        let mut board = Self {
            // Major League ties CIRAM A10 high.
            mirroring: if submapper_num == 1 {
                Mirroring::SingleScreenB
            } else {
                cart.mirroring()
            },
            submapper_num,
            prg_swapped: false,
            prg: [0; 2],
            chr: [0; 8],
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    fn update_prg_banks(&self, memory: &mut Memory) {
        let (lo, hi) = (i32::from(self.prg[0]), i32::from(self.prg[1]));
        // -2 is the second-last bank.
        let (bank_8000, bank_c000) = if self.prg_swapped { (-2, lo) } else { (lo, -2) };
        memory.map_prg(0x8000, Self::PRG_WINDOW, bank_8000, Src::PrgRom);
        memory.map_prg(0xA000, Self::PRG_WINDOW, hi, Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, bank_c000, Src::PrgRom);
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
    }
}

impl Map for IremG101 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("PRG $8000", u32::from(self.prg[0])));
        out.push(("PRG $A000", u32::from(self.prg[1])));
        out.push(("PRG swapped", u32::from(self.prg_swapped)));
        let chr = [
            "CHR 0", "CHR 1", "CHR 2", "CHR 3", "CHR 4", "CHR 5", "CHR 6", "CHR 7",
        ];
        for (name, reg) in chr.into_iter().zip(self.chr) {
            out.push((name, u32::from(reg)));
        }
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr & 0xF007 {
            0x8000..=0x8007 => {
                self.prg[0] = val & 0x1F;
                self.update_prg_banks(memory);
            }
            0x9000..=0x9007 if self.submapper_num != 1 => {
                self.prg_swapped = val & 0x02 == 0x02;
                self.mirroring = if val & 0x01 == 0x01 {
                    Mirroring::Horizontal
                } else {
                    Mirroring::Vertical
                };
                self.update_prg_banks(memory);
                memory.set_mirroring(self.mirroring);
            }
            0xA000..=0xA007 => {
                self.prg[1] = val & 0x1F;
                self.update_prg_banks(memory);
            }
            reg @ 0xB000..=0xB007 => {
                let slot = usize::from(reg & 0x07);
                self.chr[slot] = val;
                let addr = (slot * Self::CHR_WINDOW) as u16;
                memory.map_chr(addr, Self::CHR_WINDOW, i32::from(val), Src::Chr);
            }
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, Self::PRG_WINDOW, 0, Src::PrgRam);
        self.update_prg_banks(memory);
        for (slot, reg) in self.chr.iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*reg), Src::Chr);
        }
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 64K PRG-ROM (8 8K banks), 8K PRG-RAM and 64K CHR-ROM.
    fn load(submapper_num: u8) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(64 * 1024, 8 * 1024, 64 * 1024);
        cart.header.submapper_num = submapper_num;
        let mapper = IremG101::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// `$9000` bit 1 moves the `$8000` register's bank to `$C000` and fixes `$8000`.
    #[test]
    fn prg_mode_swaps_8000_and_c000() {
        let (mut mapper, mut cart) = load(0);
        write(&mut mapper, &mut cart, 0x8000, 2);
        write(&mut mapper, &mut cart, 0xA000, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 6 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 7 * 8);

        write(&mut mapper, &mut cart, 0x9000, 0x03);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 6 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 2 * 8);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
    }

    /// The board decodes A0-A2 and A12-A15, so `$B00F` is CHR register 7.
    #[test]
    fn chr_registers_decode_with_the_f007_mask() {
        let (mut mapper, mut cart) = load(0);
        write(&mut mapper, &mut cart, 0xB00F, 9);
        assert_eq!(chr_peek(&mapper, &cart, 0x1C00), 0x80 | 9);
    }

    /// Major League has one-screen mirroring and ignores `$9000`.
    #[test]
    fn submapper_1_ignores_9000() {
        let (mut mapper, mut cart) = load(1);
        write(&mut mapper, &mut cart, 0x8000, 2);
        write(&mut mapper, &mut cart, 0x9000, 0x03);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 8);
    }
}
