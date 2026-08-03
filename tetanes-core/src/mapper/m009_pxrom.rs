//! `PxROM/MMC2 (Mapper 009)`.
//!
//! <https://wiki.nesdev.org/w/index.php/MMC2>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    mapper::{self, Map, Mapper, MapperOps},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `PxROM/MMC2 (Mapper 009)`.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Pxrom {
    pub mirroring: Mirroring,
    pub prg_bank: u8,
    /// Which of the two bank registers each 4K CHR half is currently using, flipped by reads of
    /// the $FD/$FE tile addresses.
    pub latch: [usize; 2],
    /// $FD/$FE bank registers for the low half, then for the high half.
    pub latch_banks: [u8; 4],
}

impl Pxrom {
    const CHR_WINDOW: usize = 4 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const MIRRORING_MASK: u8 = 0x01;
    const PRG_WINDOW: usize = 8 * 1024;

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg_bank: 0,
            latch: [0; 2],
            latch_banks: [0; 4],
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    /// Re-map one 4K CHR window from whichever bank register its latch currently selects.
    ///
    /// The latch flips on tile fetches, so this runs thousands of times a frame; rebuilding every
    /// page table entry through `update_banks` for it cost Punch-Out!! ~20% of its frame time.
    fn update_chr_banks(&self, memory: &mut Memory, half: usize) {
        let bank = self.latch_banks[self.latch[half] + half * 2];
        memory.map_chr(
            (half * Self::CHR_WINDOW) as u16,
            Self::CHR_WINDOW,
            i32::from(bank),
            Src::Chr,
        );
    }
}

impl Map for Pxrom {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::WATCHES_PPU_BUS
    }

    /// The CHR latch is driven by which tile addresses the PPU fetches.
    fn ppu_bus_addr(&mut self, memory: &mut Memory, addr: u16) {
        if matches!(addr, 0x0FD8 | 0x0FE8 | 0x1FD8..=0x1FDF | 0x1FE8..=0x1FEF) {
            let addr = addr as usize;
            let half = addr >> 12;
            let latch = ((addr >> 4) & 0xFF) - 0xFD;
            if self.latch[half] != latch {
                self.latch[half] = latch;
                self.update_chr_banks(memory, half);
            }
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr {
            0xA000..=0xAFFF => self.prg_bank = val & 0x0F,
            0xB000..=0xEFFF => self.latch_banks[((addr - 0xB000) >> 12) as usize] = val & 0x1F,
            0xF000..=0xFFFF => {
                self.mirroring = match val & Self::MIRRORING_MASK {
                    0b00 => Mirroring::Vertical,
                    _ => Mirroring::Horizontal,
                };
            }
            _ => return,
        }
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_prg(0xA000, Self::PRG_WINDOW, -3, Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, -2, Src::PrgRom);
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
        self.update_chr_banks(memory, 0);
        self.update_chr_banks(memory, 1);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (16 8K banks), 8K PRG-RAM, 64K CHR-ROM (16 4K banks).
    fn pxrom() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 64 * 1024);
        let mapper = Pxrom::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// The four CHR bank registers, in the order MMC2 decodes them: $0000's $FD bank, $0000's $FE
    /// bank, then the same pair for $1000.
    fn set_chr_banks(mapper: &mut Mapper, cart: &mut Cart, banks: [u8; 4]) {
        for (reg, bank) in [0xB000, 0xC000, 0xD000, 0xE000].into_iter().zip(banks) {
            write(mapper, cart, reg, bank);
        }
    }

    /// The byte the first page of 4K CHR bank `bank` holds.
    fn chr(bank: u8) -> u8 {
        0x80 | (bank * 4)
    }

    /// A PPU fetch of `addr`, which is the only thing that moves a latch.
    fn fetch(mapper: &mut Mapper, cart: &mut Cart, addr: u16) {
        mapper.ppu_bus_addr(&mut cart.memory, addr);
    }

    /// One switchable 8K bank at $8000 and the last three 8K banks fixed behind it.
    #[test]
    fn powers_on_with_the_last_three_8k_banks_fixed() {
        let (mapper, cart) = pxrom();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "PRG-RAM");
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "switchable");
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 13 * 8, "third from last");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 14 * 8, "second from last");
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 15 * 8, "last");
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 127);
    }

    /// $A000-$AFFF selects the switchable bank, four bits wide.
    #[test]
    fn a000_selects_the_8k_bank_at_8000() {
        let (mut mapper, mut cart) = pxrom();
        write(&mut mapper, &mut cart, 0xA000, 5);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 5 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 13 * 8, "still fixed");

        // Bits above the low nibble are not part of the bank number.
        write(&mut mapper, &mut cart, 0xAFFF, 0xF3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 8);
    }

    /// Each 4K CHR half has two bank registers and a latch choosing between them; the latch flips
    /// on the tile fetch itself, which is what lets Punch-Out!! swap a face mid-scanline.
    #[test]
    fn the_latch_picks_which_bank_register_each_chr_half_uses() {
        let (mut mapper, mut cart) = pxrom();
        set_chr_banks(&mut mapper, &mut cart, [1, 2, 3, 4]);

        // Both latches power on at 0, i.e. the $FD register of each half.
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), chr(1), "$B000");
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), chr(3), "$D000");

        fetch(&mut mapper, &mut cart, 0x0FE8);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), chr(2), "$C000");
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), chr(3), "unmoved");

        fetch(&mut mapper, &mut cart, 0x1FE8);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), chr(4), "$E000");

        fetch(&mut mapper, &mut cart, 0x0FD8);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), chr(1), "back to $B000");
        fetch(&mut mapper, &mut cart, 0x1FD8);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), chr(3), "back to $D000");
    }

    /// MMC2's low half latches on exactly $0FD8 and $0FE8, while its high half takes the whole
    /// eight-address run - unlike MMC4, which takes the run in both halves. Getting the low half
    /// wrong is invisible on a hash: it only shows on the tile either side of the trigger.
    #[test]
    fn the_low_half_latches_on_the_exact_address_and_the_high_half_on_a_range() {
        let (mut mapper, mut cart) = pxrom();
        set_chr_banks(&mut mapper, &mut cart, [1, 2, 3, 4]);

        for addr in [0x0FD9, 0x0FDF, 0x0FE9, 0x0FEF] {
            fetch(&mut mapper, &mut cart, addr);
            assert_eq!(
                chr_peek(&mapper, &cart, 0x0000),
                chr(1),
                "${addr:04X} must not move the low latch"
            );
        }
        for addr in [0x1FE9, 0x1FEF] {
            fetch(&mut mapper, &mut cart, 0x1FD8);
            fetch(&mut mapper, &mut cart, addr);
            assert_eq!(
                chr_peek(&mapper, &cart, 0x1000),
                chr(4),
                "${addr:04X} must move the high latch"
            );
        }

        // Nothing else on the PPU bus is a trigger, including the nametable fetches that dominate
        // it.
        for addr in [0x0000, 0x0FC8, 0x1FF8, 0x2FD8] {
            fetch(&mut mapper, &mut cart, addr);
        }
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), chr(1));
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), chr(4));
    }

    /// $F000-$FFFF is the mirroring register: bit 0 clear is vertical.
    #[test]
    fn f000_selects_mirroring() {
        let (mut mapper, mut cart) = pxrom();

        write(&mut mapper, &mut cart, 0xF000, 0x00);
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
        cart.memory.chr_write(0x2000, 0x11);
        assert_eq!(chr_peek(&mapper, &cart, 0x2800), 0x11, "vertical");

        write(&mut mapper, &mut cart, 0xFFFF, 0x01);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        cart.memory.chr_write(0x2000, 0x22);
        assert_eq!(chr_peek(&mapper, &cart, 0x2400), 0x22, "horizontal");
    }

    /// $8000-$9FFF is not decoded by the board.
    #[test]
    fn writes_below_a000_are_ignored() {
        let (mut mapper, mut cart) = pxrom();
        write(&mut mapper, &mut cart, 0xA000, 5);
        for addr in [0x4100, 0x6000, 0x8000, 0x9FFF] {
            write(&mut mapper, &mut cart, addr, 2);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 5 * 8, "${addr:04X}");
        }
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// [`Bus::rebuild_mapper_state`](crate::bus::Bus::rebuild_mapper_state) relies on after a
    /// save state - including the latch, which selects a different bank register than the one a
    /// fresh board would use.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = pxrom();
        write(&mut mapper, &mut cart, 0xA000, 6);
        set_chr_banks(&mut mapper, &mut cart, [1, 2, 3, 4]);
        fetch(&mut mapper, &mut cart, 0x0FE8);
        fetch(&mut mapper, &mut cart, 0x1FE8);

        let sample = |mapper: &Mapper, cart: &Cart| -> Vec<u8> {
            [0x6000, 0x8000, 0xA000, 0xC000, 0xE000]
                .into_iter()
                .map(|addr| prg_peek(mapper, cart, addr))
                .chain(
                    [0x0000, 0x1000]
                        .into_iter()
                        .map(|addr| chr_peek(mapper, cart, addr)),
                )
                .collect()
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
    }
}
