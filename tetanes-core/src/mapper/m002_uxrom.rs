//! `UxROM` (Mapper 002)
//!
//! <https://wiki.nesdev.com/w/index.php/UxROM>

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
pub struct Uxrom {
    pub mirroring: Mirroring,
    pub prg_rom_banks: Banks,
}

impl Uxrom {
    const PRG_ROM_WINDOW: usize = 16 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        if !cart.has_chr_rom() && cart.chr_ram.is_empty() {
            cart.add_chr_ram(Self::CHR_RAM_SIZE);
        };
        let mut uxrom = Self {
            mirroring: cart.mirroring(),
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW)?,
        };
        uxrom.prg_rom_banks.set(1, uxrom.prg_rom_banks.last());
        Ok(uxrom.into())
    }
}

impl MemMap for Uxrom {
    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable
    // CPU $C000..=$FFFF 16K PRG-ROM Fixed to Last Bank

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr.into()),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Bus,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(addr.into(), val),
            0x8000..=0xFFFF => {
                self.prg_rom_banks.set(0, val.into());
                MappedWrite::Bus
            }
            _ => MappedWrite::Bus,
        }
    }
}

impl Mapped for Uxrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl Clock for Uxrom {}
impl Regional for Uxrom {}
impl Reset for Uxrom {}
impl Sram for Uxrom {}
