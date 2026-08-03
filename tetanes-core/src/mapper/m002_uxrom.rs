//! `UxROM (Mapper 002)`.
//!
//! <https://wiki.nesdev.org/w/index.php/UxROM>

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

/// `UxROM` (Mapper 002).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Uxrom {
    pub mirroring: Mirroring,
    pub prg_bank: u8,
}

impl Uxrom {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable
    // CPU $C000..=$FFFF 16K PRG-ROM Fixed to Last Bank
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Uxrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("PRG $8000", u32::from(self.prg_bank)));
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
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 256K PRG-ROM (16 16K banks), no PRG-RAM, 8K CHR-RAM. A UOROM this size is what makes the
    /// register's width visible: with only 8 banks a masked register and a wrapped one agree.
    fn uxrom() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(256 * 1024, 0, 0);
        let mapper = Uxrom::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// $C000 is fixed to the last bank from power-on, which is what puts a reset vector under the
    /// CPU before the game has written its bank register.
    #[test]
    fn powers_on_with_bank_0_low_and_the_last_bank_fixed_high() {
        let (mapper, cart) = uxrom();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 15 * 16, "last bank");
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 255);
    }

    /// Any write to $8000-$FFFF selects the 16K bank at $8000 and leaves $C000 alone.
    #[test]
    fn a_write_anywhere_above_8000_banks_only_the_low_half() {
        let (mut mapper, mut cart) = uxrom();
        for (addr, bank) in [(0x8000, 3), (0xC000, 5), (0xFFFF, 6)] {
            write(&mut mapper, &mut cart, addr, bank);
            assert_eq!(
                prg_peek(&mapper, &cart, 0x8000),
                bank * 16,
                "${addr:04X} selects bank {bank}"
            );
            assert_eq!(prg_peek(&mapper, &cart, 0xC000), 15 * 16, "still fixed");
        }
    }

    /// The board decodes only $8000-$FFFF; below that is the bus, not a register.
    #[test]
    fn writes_below_8000_are_ignored() {
        let (mut mapper, mut cart) = uxrom();
        write(&mut mapper, &mut cart, 0x8000, 3);
        for addr in [0x4100, 0x6000, 0x7FFF] {
            write(&mut mapper, &mut cart, addr, 5);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "${addr:04X}");
        }
    }

    /// The whole byte is the bank number - there is no mask - and a bank past the end of the ROM
    /// wraps within the region rather than reading outside it.
    #[test]
    fn the_register_is_a_whole_byte_and_wraps() {
        let (mut mapper, mut cart) = uxrom();
        write(&mut mapper, &mut cart, 0x8000, 9);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 9 * 16, "not masked to 8");
        write(&mut mapper, &mut cart, 0x8000, 0xFF);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 15 * 16, "255 wraps to 15");
    }

    /// UxROM has no CHR banking at all - the 8K window is CHR-RAM the game writes through.
    #[test]
    fn chr_is_a_fixed_8k_window() {
        let (mut mapper, mut cart) = uxrom();
        write(&mut mapper, &mut cart, 0x8000, 4);
        cart.memory.chr_write(0x0000, 0x33);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0000),
            0x33,
            "CHR-RAM is writable"
        );
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x84, "still CHR page 4");
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// [`Bus::rebuild_mapper_state`](crate::bus::Bus::rebuild_mapper_state) relies on after a
    /// save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = uxrom();
        write(&mut mapper, &mut cart, 0x8000, 5);

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
