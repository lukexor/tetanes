//! CNROM (Mapper 003)
//!
//! <https://wiki.nesdev.com/w/index.php/CNROM>
//! <https://wiki.nesdev.com/w/index.php/INES_Mapper_003>

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
pub struct Cnrom {
    pub mirroring: Mirroring,
    pub chr_banks: Banks,
    pub mirror_prg_rom: bool,
}

impl Cnrom {
    const CHR_ROM_WINDOW: usize = 8 * 1024;

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let cnrom = Self {
            mirroring: cart.mirroring(),
            chr_banks: Banks::new(0x0000, 0x1FFFF, cart.chr_rom.len(), Self::CHR_ROM_WINDOW)?,
            mirror_prg_rom: cart.prg_rom.len() <= 0x4000,
        };
        Ok(cnrom.into())
    }
}

impl Mapped for Cnrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MemMap for Cnrom {
    // PPU $0000..=$1FFF 8K CHR-ROM Banks Switchable
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Fixed
    // CPU $C000..=$FFFF 16K PRG-ROM Bank Fixed or Bank 1 Mirror if only 16 KB PRG-ROM

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x8000..=0xBFFF => MappedRead::PrgRom((addr & 0x3FFF).into()),
            0xC000..=0xFFFF => {
                let mirror = if self.mirror_prg_rom { 0x3FFF } else { 0x7FFF };
                MappedRead::PrgRom((addr & mirror).into())
            }
            _ => MappedRead::Bus,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        if matches!(addr, 0x8000..=0xFFFF) {
            self.chr_banks.set(0, val.into());
        }
        MappedWrite::Bus
    }
}

impl Reset for Cnrom {}
impl Clock for Cnrom {}
impl Regional for Cnrom {}
impl Sram for Cnrom {}
