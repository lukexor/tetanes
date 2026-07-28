//! `NROM` (Mapper 000).
//!
//! <https://wiki.nesdev.org/w/index.php/NROM>

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

/// `NROM` (Mapper 000).
///
/// The board has no registers at all - the entire cartridge is fixed at power-on - so it holds no
/// state and exists only to configure the page tables in [`Memory::map_prg`]/[`Memory::map_chr`]
/// when the cart is loaded.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nrom {
    pub mirroring: Mirroring,
}

impl Nrom {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    /// Load `Nrom` from `Cart`.
    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $6000..=$7FFF 2K or 4K PRG-RAM Family Basic only. 8K is provided by default.
    // CPU $8000..=$BFFF 16K PRG-ROM Bank 1 for NROM128 or NROM256
    // CPU $C000..=$FFFF 16K PRG-ROM Bank 2 for NROM256 or Bank 1 Mirror for NROM128
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        // NROM-128 has a single 16K bank mirrored into both slots, which falls out of the bank
        // index wrapping within the region rather than needing a `mirror_prg_rom` flag.
        let mut board = Self {
            mirroring: cart.mirroring(),
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Nrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, 8 * 1024, 0, Src::PrgRam);
        memory.map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
        // NROM-128 has a single 16K bank mirrored into both slots, which falls out of the bank
        // index wrapping within the region rather than needing a `mirror_prg_rom` flag.
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// `prg_rom` bytes of PRG-ROM (16K or 32K for a real NROM), 8K PRG-RAM, 8K CHR-ROM.
    fn load(prg_rom: usize) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(prg_rom, 8 * 1024, 8 * 1024);
        let mapper = Nrom::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    fn nrom_256() -> (Mapper, Cart) {
        load(32 * 1024)
    }

    /// NROM-256's two 16K halves are both fixed, and PRG-RAM sits under $6000.
    #[test]
    fn an_nrom_256_cart_maps_both_16k_halves_and_its_prg_ram() {
        let (mapper, cart) = nrom_256();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "first half");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16, "second half");
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 31, "last page");
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "PRG-RAM");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80, "8K of CHR");
        assert_eq!(chr_peek(&mapper, &cart, 0x1FFF), 0x87);
    }

    /// NROM-128 has one 16K bank in both slots. That is not a `mirror_prg_rom` flag - it falls out
    /// of the bank index wrapping within a region only one bank long, so the reset vectors at
    /// $FFFA are the same bytes as $BFFA.
    #[test]
    fn an_nrom_128_cart_mirrors_its_single_bank_into_both_slots() {
        let (mapper, cart) = load(16 * 1024);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 0, "the same bank again");
        assert_eq!(prg_peek(&mapper, &cart, 0xBFFF), 15);
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 15, "mirrored reset vectors");
    }

    /// The board has no registers at all, so a write anywhere in cart space must leave the layout
    /// exactly as the cart loaded it.
    #[test]
    fn writes_reach_no_register() {
        let (mut mapper, mut cart) = nrom_256();
        for addr in [0x4100, 0x6000, 0x8000, 0xA000, 0xC000, 0xFFFF] {
            write(&mut mapper, &mut cart, addr, 0x03);
        }
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80);
        // $6000 is PRG-RAM and takes the write like any other RAM.
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x03);
    }

    /// Mirroring is hard-wired by the header's solder pad, and must reach CIRAM rather than only
    /// being reported by `mirroring()`.
    #[test]
    fn header_mirroring_reaches_ciram() {
        let mut cart = page_indexed_cart(32 * 1024, 8 * 1024, 8 * 1024);
        cart.header.flags |= 0x01; // vertical
        let mapper = Nrom::load(&mut cart).expect("valid mapper");
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);

        cart.memory.chr_write(0x2000, 0x11);
        assert_eq!(chr_peek(&mapper, &cart, 0x2800), 0x11, "vertical");
        assert_ne!(chr_peek(&mapper, &cart, 0x2400), 0x11);
    }

    /// `update_banks` must rebuild every window from the board alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = nrom_256();
        let sample = |mapper: &Mapper, cart: &Cart| {
            [
                prg_peek(mapper, cart, 0x6000),
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
