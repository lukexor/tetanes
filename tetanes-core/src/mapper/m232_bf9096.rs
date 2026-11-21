//! `BF9096` (Mapper 232).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_232>

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

/// `BF9096` (Mapper 232), the Camerica Quattro multicart board.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bf9096 {
    /// The hard-wired nametable arrangement.
    pub mirroring: Mirroring,
    /// Submapper 1, the Aladdin Deck Enhancer, swaps the two block bits.
    pub submapper_num: u8,
    /// The selected 64K block.
    pub block: u8,
    /// The 16K page at $8000 within the block.
    pub page: u8,
}

impl Bf9096 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    const PAGES_PER_BLOCK: u8 = 4;

    // PPU $0000..=$1FFF 8K fixed CHR-RAM
    // CPU $8000..=$BFFF 16K PRG-ROM page within the selected 64K block
    // CPU $C000..=$FFFF 16K PRG-ROM fixed to the block's last page
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            submapper_num: cart.submapper_num(),
            block: 0,
            page: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Bf9096 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("Block", u32::from(self.block)));
        out.push(("Page", u32::from(self.page)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr {
            0x8000..=0xBFFF => {
                self.block = if self.submapper_num == 1 {
                    ((val >> 4) & 0x01) | ((val >> 2) & 0x02)
                } else {
                    (val >> 3) & 0x03
                };
                self.update_banks(memory);
            }
            0xC000..=0xFFFF => {
                self.page = val & 0x03;
                self.update_banks(memory);
            }
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        let base = self.block * Self::PAGES_PER_BLOCK;
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(base | self.page),
            Src::PrgRom,
        );
        memory.map_prg(
            0xC000,
            Self::PRG_WINDOW,
            i32::from(base | (Self::PAGES_PER_BLOCK - 1)),
            Src::PrgRom,
        );
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{page_indexed_cart, prg_peek, write};

    /// 256K of PRG-ROM is past what page-indexed bytes can name, so this marks each 16K bank with
    /// its own number instead.
    fn load(submapper_num: u8) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(16 * 1024, 0, 0);
        cart.memory = Memory::new(crate::memory::MemoryLayout {
            prg_rom: 256 * 1024,
            chr: 8 * 1024,
            chr_writable: true,
            ..Default::default()
        });
        for (bank, data) in cart
            .memory
            .region_mut(Src::PrgRom)
            .chunks_mut(Bf9096::PRG_WINDOW)
            .enumerate()
        {
            data.fill(bank as u8);
        }
        cart.header.submapper_num = submapper_num;
        let mapper = Bf9096::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// $8000 picks the 64K block, $C000 the page within it, and $C000 shows the block's last page.
    #[test]
    fn block_and_page_select_the_bank() {
        let (mut mapper, mut cart) = load(0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 3, "block 0's last page");
        write(&mut mapper, &mut cart, 0x8000, 0x10);
        write(&mut mapper, &mut cart, 0xC000, 0x01);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 4 + 1);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 2 * 4 + 3);
    }

    /// Submapper 1 wires the block bits the other way round.
    #[test]
    fn aladdin_swaps_the_block_bits() {
        let (mut mapper, mut cart) = load(1);
        write(&mut mapper, &mut cart, 0x8000, 0x10);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 4 + 3, "block 1");
    }
}
