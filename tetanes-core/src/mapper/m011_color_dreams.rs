//! `Color Dreams (Mapper 011)`.
//!
//! <https://wiki.nesdev.org/w/index.php/Color_Dreams>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
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
        board.sync(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for ColorDreams {
    fn uses_page_tables(&self) -> bool {
        true
    }

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

    fn sync(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
    }
}

impl Reset for ColorDreams {}
impl Clock for ColorDreams {}
impl Regional for ColorDreams {}
impl Sram for ColorDreams {}
