//! `GxROM (Mapper 066)`.
//!
//! <https://wiki.nesdev.org/w/index.php/GxROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `GxROM` (Mapper 066).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Gxrom {
    pub mirroring: Mirroring,
    pub chr_bank: u8,
    pub prg_bank: u8,
}

impl Gxrom {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    const CHR_BANK_MASK: u8 = 0x0F;
    const PRG_BANK_MASK: u8 = 0x30;

    // PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            chr_bank: 0,
            prg_bank: 0,
        };
        board.sync(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Gxrom {

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            self.chr_bank = val & Self::CHR_BANK_MASK;
            self.prg_bank = (val & Self::PRG_BANK_MASK) >> 4;
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
        memory.set_mirroring(self.mirroring);
    }
}

impl Reset for Gxrom {}
impl Clock for Gxrom {}
impl Regional for Gxrom {}
impl Sram for Gxrom {}
