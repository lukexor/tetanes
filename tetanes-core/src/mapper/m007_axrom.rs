//! `AxROM (Mapper 007)`.
//!
//! <https://wiki.nesdev.org/w/index.php/AxROM>

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
