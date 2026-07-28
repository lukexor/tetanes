//! `NINA-001 (Mapper 034)`.
//!
//! <https://wiki.nesdev.org/w/index.php/INES_Mapper_034>

use crate::{
    cart::Cart,
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `NINA-001` (Mapper 034).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nina001 {
    pub mirroring: Mirroring,
    pub prg_bank: u8,
    pub chr_banks: [u8; 2],
}

impl Nina001 {
    const PRG_WINDOW: usize = 32 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;

    // PPU $0000..=$0FFF 4K CHR-ROM Bank Switchable
    // PPU $1000..=$1FFF 4K CHR-ROM Bank Switchable
    // CPU $6000..=$7FFF 8K PRG-RAM, with registers at $7FFD..=$7FFF
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: Mirroring::Horizontal,
            prg_bank: 0,
            chr_banks: [0, 1],
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Nina001 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        // The register writes also land in PRG-RAM, which the caller has already stored.
        match addr {
            0x7FFD => {
                self.prg_bank = val & 0x01;
                memory.map_prg(
                    0x8000,
                    Self::PRG_WINDOW,
                    i32::from(self.prg_bank),
                    Src::PrgRom,
                );
            }
            0x7FFE => {
                self.chr_banks[0] = val & 0x0F;
                memory.map_chr(
                    0x0000,
                    Self::CHR_WINDOW,
                    i32::from(self.chr_banks[0]),
                    Src::Chr,
                );
            }
            0x7FFF => {
                self.chr_banks[1] = val & 0x0F;
                memory.map_chr(
                    0x1000,
                    Self::CHR_WINDOW,
                    i32::from(self.chr_banks[1]),
                    Src::Chr,
                );
            }
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_chr(
            0x0000,
            Self::CHR_WINDOW,
            i32::from(self.chr_banks[0]),
            Src::Chr,
        );
        memory.map_chr(
            0x1000,
            Self::CHR_WINDOW,
            i32::from(self.chr_banks[1]),
            Src::Chr,
        );
        memory.set_mirroring(self.mirroring);
    }
}
