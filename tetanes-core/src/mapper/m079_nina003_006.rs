//! `NINA-003/NINA-006 (Mapper 079)`.
//!
//! <https://wiki.nesdev.org/w/index.php/INES_Mapper_079>

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
