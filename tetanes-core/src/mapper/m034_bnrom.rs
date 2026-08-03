//! `BNROM (Mapper 034)`.
//!
//! <https://wiki.nesdev.org/w/index.php/BNROM>

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

/// `BNROM` (Mapper 034).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bnrom {
    pub mirroring: Mirroring,
    pub prg_bank: u8,
}

impl Bnrom {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

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

impl Map for Bnrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            self.prg_bank = val;
            memory.map_prg(0x8000, Self::PRG_WINDOW, i32::from(val), Src::PrgRom);
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

    /// 128K PRG-ROM (4 32K banks), no PRG-RAM, 8K CHR-RAM - which is what tells mapper 034 apart
    /// from NINA-001, whose CHR-ROM is at least 16K.
    fn bnrom() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 0);
        let mapper = Bnrom::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// The whole $8000-$FFFF window is one switchable 32K bank, so the reset vectors move with it.
    #[test]
    fn a_write_above_8000_banks_the_whole_32k_window() {
        let (mut mapper, mut cart) = bnrom();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16, "same 32K bank");

        write(&mut mapper, &mut cart, 0xFFFF, 2);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 32);
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 2 * 32 + 31);
    }

    /// The whole byte is the bank number - there is no mask and no other register - so a bank past
    /// the end wraps within the region.
    #[test]
    fn the_register_is_a_whole_byte_and_wraps() {
        let (mut mapper, mut cart) = bnrom();
        write(&mut mapper, &mut cart, 0x8000, 0xFF);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 32, "255 wraps to 3");
    }

    /// The board decodes only $8000-$FFFF; below that is the bus, not a register.
    #[test]
    fn writes_below_8000_are_ignored() {
        let (mut mapper, mut cart) = bnrom();
        write(&mut mapper, &mut cart, 0x8000, 2);
        for addr in [0x4100, 0x6000, 0x7FFF] {
            write(&mut mapper, &mut cart, addr, 1);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 32, "${addr:04X}");
        }
    }

    /// BNROM has no CHR banking; the 8K window is CHR-RAM the game writes through.
    #[test]
    fn chr_is_a_fixed_8k_window() {
        let (mut mapper, mut cart) = bnrom();
        write(&mut mapper, &mut cart, 0x8000, 3);
        cart.memory.chr_write(0x0000, 0x33);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0000),
            0x33,
            "CHR-RAM is writable"
        );
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x84, "still page 4");
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// [`Bus::rebuild_mapper_state`](crate::bus::Bus::rebuild_mapper_state) relies on after a
    /// save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = bnrom();
        write(&mut mapper, &mut cart, 0x8000, 2);

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
    }
}
