//! `CPROM` (Mapper 013).
//!
//! <https://www.nesdev.org/wiki/CPROM>

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

/// `CPROM` (Mapper 013).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Cprom {
    /// The header's nametable arrangement, which the board hard-wires.
    pub mirroring: Mirroring,
    /// `$8000-$FFFF` bits 0-1: the 4K CHR-RAM bank at `$1000`.
    pub chr_bank: u8,
}

impl Cprom {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;

    // PPU $0000..=$0FFF 4K CHR-RAM fixed to the first bank
    // PPU $1000..=$1FFF 4K switchable CHR-RAM bank
    // CPU $8000..=$FFFF 32K PRG-ROM
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            chr_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Cprom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("CHR $1000", u32::from(self.chr_bank)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            // The ROM drives the data bus too, so the latch gets the AND of both.
            self.chr_bank = val & memory.prg_peek(addr) & 0x03;
            self.update_banks(memory);
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.map_chr(0x1000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mapper::test_utils::{chr_peek, page_indexed_cart, write},
        memory::{MemoryLayout, PAGE_SIZE},
    };

    /// 32K PRG-ROM, with each page holding its index, and 16K CHR-RAM, each page holding
    /// `0x80 | index`.
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(32 * 1024, 0, 0);
        let prg = cart.memory.region_mut(Src::PrgRom).to_vec();
        cart.memory = Memory::new(MemoryLayout {
            prg_rom: 32 * 1024,
            prg_ram: 0,
            battery_ext: 0,
            chr: 16 * 1024,
            chr_writable: true,
            ciram: 2 * 1024,
            ex_ram: 0,
        });
        cart.memory.region_mut(Src::PrgRom).copy_from_slice(&prg);
        for (page, data) in cart
            .memory
            .region_mut(Src::Chr)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            data.fill(0x80 | page as u8);
        }
        let mapper = Cprom::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// `$0000` stays on the first 4K, and the register banks `$1000`, through bus conflicts.
    #[test]
    fn switches_the_upper_4k_with_bus_conflicts() {
        let (mut mapper, mut cart) = load();
        // $8C00 is PRG page 3, so it reads $03.
        write(&mut mapper, &mut cart, 0x8C00, 0x02);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80, "fixed");
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | 8, "4K bank 2");

        write(&mut mapper, &mut cart, 0x8C00, 0x03);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | 12, "4K bank 3");

        // $8000 is PRG page 0, so it reads $00 and the AND clears the bank.
        write(&mut mapper, &mut cart, 0x8000, 0x03);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80, "4K bank 0");
    }
}
