//! `NINA-003`/`NINA-006` (Mapper 079)
//!
//! <https://www.nesdev.org/wiki/NINA-001>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Mapped, MappedRead, MappedWrite, Mapper, MemMap},
    mem::Banks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nina003006 {
    pub mirroring: Mirroring,
    pub mapper_num: u16,
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl Nina003006 {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const CHR_ROM_WINDOW: usize = 8 * 1024;

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let nina003006 = Self {
            mirroring: cart.mirroring(),
            mapper_num: cart.mapper_num(),
            chr_banks: Banks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_ROM_WINDOW)?,
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW)?,
        };
        Ok(nina003006.into())
    }
}

impl Mapped for Nina003006 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MemMap for Nina003006 {
    // PPU $0000..=$1FFF 8K switchable CHR ROM bank
    // CPU $8000..=$FFFF 32K switchable PRG ROM bank

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Bus,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        if matches!(addr, 0x0000..=0x1FFF) {
            // return MappedWrite::Chr(self.chr_banks.translate(addr), val);
        } else if (addr & 0xE100) == 0x4100 {
            if self.mapper_num == 113 {
                self.prg_rom_banks.set(0, ((val >> 3) & 0x07).into());
                self.chr_banks
                    .set(0, ((val & 0x07) | ((val >> 3) & 0x08)).into());
                self.set_mirroring(if val & 0x80 == 0x80 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                });
            } else {
                self.prg_rom_banks.set(0, ((val >> 3) & 0x01).into());
                self.chr_banks.set(0, (val & 0x07).into());
            }
        }
        MappedWrite::Bus
    }
}

impl Reset for Nina003006 {}
impl Clock for Nina003006 {}
impl Regional for Nina003006 {}
impl Sram for Nina003006 {}
