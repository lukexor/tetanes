//! `GxROM` (Mapper 066).
//!
//! <https://wiki.nesdev.org/w/index.php?title=GxROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{
        self, MapRead, MapWrite, MappedRead, MappedWrite, Mapper, Mirrored, OnBusRead, OnBusWrite,
    },
    mem::Banks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `GxROM` (Mapper 066).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Gxrom {
    pub mirroring: Mirroring,
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl Gxrom {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    const CHR_BANK_MASK: u8 = 0x0F; // 0b1111
    const PRG_BANK_MASK: u8 = 0x30; // 0b110000

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let gxrom = Self {
            mirroring: cart.mirroring(),
            chr_banks: Banks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_WINDOW)?,
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW)?,
        };
        Ok(gxrom.into())
    }
}

impl Mirrored for Gxrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MapRead for Gxrom {
    // PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Bus,
        }
    }
}

impl MapWrite for Gxrom {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        if matches!(addr, 0x8000..=0xFFFF) {
            self.chr_banks.set(0, (val & Self::CHR_BANK_MASK).into());
            self.prg_rom_banks
                .set(0, ((val & Self::PRG_BANK_MASK) >> 4).into());
        }
        MappedWrite::Bus
    }
}

impl OnBusRead for Gxrom {}
impl OnBusWrite for Gxrom {}
impl Reset for Gxrom {}
impl Clock for Gxrom {}
impl Regional for Gxrom {}
impl Sram for Gxrom {}
