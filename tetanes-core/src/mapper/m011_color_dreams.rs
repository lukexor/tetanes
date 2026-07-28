//! `Color Dreams (Mapper 011)`.
//!
//! <https://wiki.nesdev.org/w/index.php/Color_Dreams>

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

/// `Color Dreams` (Mapper 011).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct ColorDreams {
    pub mapper_num: u16,
    pub mirroring: Mirroring,
    pub chr_bank: u8,
    pub prg_bank: u8,
}

impl ColorDreams {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    const CHR_BANK_MASK: u8 = 0b1111_0000;
    const PRG_BANK_MASK: u8 = 0b0000_0011;

    // PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mapper_num: cart.mapper_num(),
            mirroring: cart.mirroring(),
            chr_bank: 0,
            prg_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for ColorDreams {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, mut val: u8) {
        if addr >= 0x8000 {
            if self.mapper_num == 144 {
                // Mapper 144 ORs in the low bit of whatever the bus was already reading there.
                val |= memory.prg_peek(addr) & 0x01;
            }
            self.chr_bank = (val & Self::CHR_BANK_MASK) >> 4;
            self.prg_bank = val & Self::PRG_BANK_MASK;
            memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
            memory.map_prg(
                0x8000,
                Self::PRG_WINDOW,
                i32::from(self.prg_bank),
                Src::PrgRom,
            );
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (4 32K banks), no PRG-RAM, 64K CHR-ROM (8 8K banks).
    fn load(mapper_num: u16) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 64 * 1024);
        cart.header.mapper_num = mapper_num;
        let mapper = ColorDreams::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    fn color_dreams() -> (Mapper, Cart) {
        load(11)
    }

    /// One byte holds both banks: CHR in the high nibble, PRG in the low two bits.
    #[test]
    fn one_write_sets_both_banks_from_opposite_ends_of_the_byte() {
        let (mut mapper, mut cart) = color_dreams();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "bank 0 at power-on");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80);

        write(&mut mapper, &mut cart, 0x8000, 0x32);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 32, "PRG bank 2");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 2 * 32 + 16, "one 32K bank");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (3 * 8), "CHR bank 3");
    }

    /// Bits 2 and 3 belong to neither register.
    #[test]
    fn the_middle_bits_are_not_part_of_either_bank() {
        let (mut mapper, mut cart) = color_dreams();
        write(&mut mapper, &mut cart, 0x8000, 0x1F);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 32, "PRG is 2 bits");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 8, "CHR is a nibble");
    }

    /// The board decodes only $8000-$FFFF; below that is the bus, not a register.
    #[test]
    fn writes_below_8000_are_ignored() {
        let (mut mapper, mut cart) = color_dreams();
        write(&mut mapper, &mut cart, 0x8000, 0x32);
        for addr in [0x4100, 0x6000, 0x7FFF] {
            write(&mut mapper, &mut cart, addr, 0x00);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 32, "${addr:04X}");
        }
    }

    /// Mapper 144 is a Color Dreams board with a resistor between CPU D0 and the mapper, so the
    /// ROM's own bit 0 wins the bus conflict.
    ///
    /// NB this is not the general formula the wiki gives for the board,
    /// `EffectiveData = ROM[addr] & (WrittenData | 1)`, which ANDs *every* bit with the ROM; we OR
    /// in bit 0 alone. The two agree whenever a game writes a value to an address holding that
    /// same value - which is what a cart with bus conflicts has to do - and there is no Death Race
    /// ROM here to settle the rest, so this pins what the board does today rather than blessing it.
    #[test]
    fn mapper_144_takes_bit_0_from_the_rom_data_bus() {
        // $8400 is the second 1K page of PRG bank 0, which `page_indexed_cart` fills with 0x01.
        let (mut mapper, mut cart) = load(144);
        assert_eq!(prg_peek(&mapper, &cart, 0x8400), 0x01, "ROM has bit 0 set");

        write(&mut mapper, &mut cart, 0x8400, 0x20);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 32, "0x20 | 0x01 = PRG 1");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (2 * 8), "CHR 2");

        // Plain mapper 11 has no such conflict and takes the written value as-is.
        let (mut mapper, mut cart) = color_dreams();
        write(&mut mapper, &mut cart, 0x8400, 0x20);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "PRG 0");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (2 * 8), "CHR 2");
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = color_dreams();
        write(&mut mapper, &mut cart, 0x8000, 0x32);

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
