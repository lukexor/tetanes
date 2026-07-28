//! `AxROM (Mapper 007)`.
//!
//! <https://wiki.nesdev.org/w/index.php/AxROM>

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

/// `AxROM` (Mapper 007).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Axrom {
    pub mirroring: Mirroring,
    pub prg_bank: u8,
}

impl Axrom {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    const SINGLE_SCREEN_B: u8 = 0b10000;

    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Axrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            self.prg_bank = val & 0x0F;
            memory.map_prg(
                0x8000,
                Self::PRG_WINDOW,
                i32::from(self.prg_bank),
                Src::PrgRom,
            );
            self.mirroring = if val & Self::SINGLE_SCREEN_B == Self::SINGLE_SCREEN_B {
                Mirroring::SingleScreenB
            } else {
                Mirroring::SingleScreenA
            };
            memory.set_mirroring(self.mirroring);
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
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
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (4 32K banks), no PRG-RAM, 8K CHR-RAM.
    fn axrom() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 0);
        let mapper = Axrom::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// The whole $8000-$FFFF window is one switchable 32K bank - there is no fixed half, so the
    /// reset vectors move with the bank.
    #[test]
    fn the_whole_window_is_one_switchable_32k_bank() {
        let (mut mapper, mut cart) = axrom();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16, "same 32K bank");

        write(&mut mapper, &mut cart, 0x8000, 2);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 32);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 2 * 32 + 16);
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 2 * 32 + 31);
    }

    /// Bit 4 is the mirroring bit, not the top of the bank number.
    ///
    /// The `& 0x0F` the board applies is not itself observable - no AxROM has more than 16 32K
    /// banks, and below that masking and wrapping are the same map - so what this pins is that a
    /// mirroring write does not drag the bank with it.
    #[test]
    fn bit_4_is_not_part_of_the_bank_number() {
        let (mut mapper, mut cart) = axrom();
        // 0x13 is bank 3 with single-screen B, not bank 0x13.
        write(&mut mapper, &mut cart, 0x8000, 0x13);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 32);
        // 4 banks exist, so the rest of the nibble wraps within the region.
        write(&mut mapper, &mut cart, 0x8000, 0x0F);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 32, "15 wraps to 3");
    }

    /// AxROM ignores the header's mirroring entirely: bit 4 of every bank write picks one of the
    /// two nametables, which is how its games scroll a single screen.
    #[test]
    fn bit_4_selects_the_single_screen_nametable() {
        let (mut mapper, mut cart) = axrom();

        write(&mut mapper, &mut cart, 0x8000, 0x00);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenA);
        cart.memory.chr_write(0x2000, 0xAA);
        for nt in [0x2400, 0x2800, 0x2C00] {
            assert_eq!(chr_peek(&mapper, &cart, nt), 0xAA, "one-screen A");
        }

        write(&mut mapper, &mut cart, 0x8000, 0x10);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB);
        cart.memory.chr_write(0x2000, 0xBB);
        for nt in [0x2400, 0x2800, 0x2C00] {
            assert_eq!(chr_peek(&mapper, &cart, nt), 0xBB, "one-screen B");
        }

        // The two screens are separate CIRAM banks, so switching back finds the old contents.
        write(&mut mapper, &mut cart, 0x8000, 0x00);
        assert_eq!(chr_peek(&mapper, &cart, 0x2000), 0xAA);
    }

    /// The board decodes only $8000-$FFFF; below that is the bus, not a register.
    #[test]
    fn writes_below_8000_are_ignored() {
        let (mut mapper, mut cart) = axrom();
        write(&mut mapper, &mut cart, 0x8000, 2);
        for addr in [0x4100, 0x6000, 0x7FFF] {
            write(&mut mapper, &mut cart, addr, 1);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 32, "${addr:04X}");
        }
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = axrom();
        write(&mut mapper, &mut cart, 0x8000, 0x12);

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
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB, "mirroring too");
    }
}
