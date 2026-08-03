//! `NINA-003/NINA-006 (Mapper 079)`.
//!
//! <https://wiki.nesdev.org/w/index.php/INES_Mapper_079>

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

/// `NINA-003`/`NINA-006` (Mapper 079).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nina003006 {
    pub mapper_num: u16,
    pub mirroring: Mirroring,
    pub chr_bank: u8,
    pub prg_bank: u8,
}

impl Nina003006 {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable
    // Registers are decoded at $4100..=$5FFF with mask $E100.
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

impl Map for Nina003006 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if (addr & 0xE100) != 0x4100 {
            return;
        }
        if self.mapper_num == 113 {
            self.prg_bank = (val >> 3) & 0x07;
            self.chr_bank = (val & 0x07) | ((val >> 3) & 0x08);
            self.mirroring = if val & 0x80 == 0x80 {
                Mirroring::Vertical
            } else {
                Mirroring::Horizontal
            };
            memory.set_mirroring(self.mirroring);
        } else {
            self.prg_bank = (val >> 3) & 0x01;
            self.chr_bank = val & 0x07;
        }
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
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

    /// 128K PRG-ROM (4 32K banks), no PRG-RAM, 128K CHR-ROM (16 8K banks).
    fn load(mapper_num: u16) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 128 * 1024);
        cart.header.mapper_num = mapper_num;
        let mapper = Nina003006::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    fn nina003006() -> (Mapper, Cart) {
        load(79)
    }

    /// The byte the first page of 8K CHR bank `bank` holds.
    fn chr(bank: u8) -> u8 {
        0x80 | (bank * 8)
    }

    /// The one register lives on the expansion bus, decoded with a mask rather than a range: any
    /// address with A14 and A8 set and A13 clear. Games use $4100.
    #[test]
    fn the_register_is_decoded_at_4100_by_mask() {
        let (mut mapper, mut cart) = nina003006();

        for addr in [0x4100, 0x4300, 0x5100, 0x5FFF] {
            write(&mut mapper, &mut cart, addr, 0x03);
            assert_eq!(chr_peek(&mapper, &cart, 0x0000), chr(3), "${addr:04X} hits");
            write(&mut mapper, &mut cart, addr, 0x00);
        }
        for addr in [0x4200, 0x4600, 0x6100, 0x8100] {
            write(&mut mapper, &mut cart, addr, 0x03);
            assert_eq!(
                chr_peek(&mapper, &cart, 0x0000),
                chr(0),
                "${addr:04X} misses"
            );
        }
    }

    /// NINA-003/006 splits the byte into one PRG bit and three CHR bits. Mapper 146 is the same
    /// board under another number.
    #[test]
    fn one_prg_bit_and_three_chr_bits() {
        for mapper_num in [79, 146] {
            let (mut mapper, mut cart) = load(mapper_num);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "mapper {mapper_num}");

            write(&mut mapper, &mut cart, 0x4100, 0x0D); // 0b0000_1101
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 32, "PRG bank 1");
            assert_eq!(prg_peek(&mapper, &cart, 0xC000), 48, "one 32K bank");
            assert_eq!(chr_peek(&mapper, &cart, 0x0000), chr(5), "CHR bank 5");

            // Everything above bit 3 belongs to neither register on this board.
            write(&mut mapper, &mut cart, 0x4100, 0xF0);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "PRG is one bit");
            assert_eq!(chr_peek(&mapper, &cart, 0x0000), chr(0), "CHR is three");
        }
    }

    /// Mapper 113 is the same silicon wired to a bigger cart: PRG grows to three bits, CHR takes a
    /// fourth bit from bit 6, and bit 7 becomes a mirroring control the other numbers do not have.
    #[test]
    fn mapper_113_widens_both_banks_and_adds_mirroring() {
        let (mut mapper, mut cart) = load(113);

        write(&mut mapper, &mut cart, 0x4100, 0x1D); // 0b0001_1101
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 32, "PRG bank 3");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), chr(5), "CHR bank 5");

        // Bit 6 is CHR A16, so it lands above the three low CHR bits rather than beside them.
        write(&mut mapper, &mut cart, 0x4100, 0x45); // 0b0100_0101
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), chr(13), "CHR bank 8 | 5");

        write(&mut mapper, &mut cart, 0x4100, 0x80);
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
        cart.memory.chr_write(0x2000, 0x11);
        assert_eq!(chr_peek(&mapper, &cart, 0x2800), 0x11, "vertical");

        write(&mut mapper, &mut cart, 0x4100, 0x00);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        cart.memory.chr_write(0x2000, 0x22);
        assert_eq!(chr_peek(&mapper, &cart, 0x2400), 0x22, "horizontal");
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// [`Bus::rebuild_mapper_state`](crate::bus::Bus::rebuild_mapper_state) relies on after a
    /// save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = load(113);
        write(&mut mapper, &mut cart, 0x4100, 0x9D);

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
        assert_eq!(mapper.mirroring(), Mirroring::Vertical, "mirroring too");
    }
}
