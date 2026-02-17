//! `DxROM`/`NAMCOT-3446` (Mapper 076).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_076>
//! <https://www.nesdev.org/wiki/DxROM>

use crate::{
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sram},
    fs,
    mapper::{self, BusKind, Dxrom206, Map, MappedRead, MappedWrite, Mapper},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `DxROM`/`NAMCOT-3446` (Mapper 076).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Dxrom {
    pub inner: Dxrom206,
}

impl Dxrom {
    const CHR_WINDOW: usize = 2048;

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let dxrom = Self {
            inner: Dxrom206::new(cart, Self::CHR_WINDOW)?,
        };
        Ok(dxrom.into())
    }

    pub fn update_chr_banks(&mut self) {
        self.inner.set_chr_banks(|banks, regs| {
            banks.set(0, regs[2] as usize);
            banks.set(1, regs[3] as usize);
            banks.set(2, regs[4] as usize);
            banks.set(3, regs[5] as usize);
        });
    }
}

impl Map for Dxrom {
    // PPU $0000..=$07FF (or $1000..=$17FF) 2K CHR-ROM/RAM Bank 1 Switchable
    // PPU $0800..=$0FFF (or $1800..=$1FFF) 2K CHR-ROM/RAM Bank 2 Switchable
    // PPU $1000..=$17FF (or $0000..=$07FF) 2K CHR-ROM/RAM Bank 3 Switchable
    // PPU $1800..=$1FFF (or $0800..=$0FFF) 2K CHR-ROM/RAM Bank 4 Switchable

    // CPU $8000..=$9FFF (or $C000..=$DFFF) 8K PRG-ROM Bank 1 Switchable
    // CPU $A000..=$BFFF 8K PRG-ROM Bank 2 Switchable
    // CPU $C000..=$DFFF (or $8000..=$9FFF) 8K PRG-ROM Bank 3 Fixed to second-to-last Bank
    // CPU $E000..=$FFFF 8K PRG-ROM Bank 4 Fixed to Last

    fn map_read(&mut self, addr: u16) -> MappedRead {
        self.inner.map_read(addr)
    }

    fn map_peek(&self, addr: u16) -> MappedRead {
        self.inner.map_peek(addr)
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        let write = self.inner.map_write(addr, val);
        if matches!(addr, 0x8000..=0x8001) {
            self.update_chr_banks();
        }
        write
    }

    fn bus_read(&mut self, addr: u16, kind: BusKind) {
        self.inner.bus_read(addr, kind)
    }

    fn bus_write(&mut self, addr: u16, val: u8, kind: BusKind) {
        self.inner.bus_write(addr, val, kind)
    }

    fn mirroring(&self) -> Mirroring {
        self.inner.mirroring()
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.inner.set_mirroring(mirroring);
    }
}

impl Reset for Dxrom {
    fn reset(&mut self, kind: ResetKind) {
        self.inner.reset(kind);
        self.update_chr_banks();
    }
}
impl Clock for Dxrom {
    fn clock(&mut self) {
        self.inner.clock();
    }
}
impl Regional for Dxrom {
    fn region(&self) -> NesRegion {
        self.inner.region()
    }

    fn set_region(&mut self, region: NesRegion) {
        self.inner.set_region(region)
    }
}
impl Sram for Dxrom {
    fn save(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        self.inner.save(path)
    }

    fn load(&mut self, path: impl AsRef<Path>) -> fs::Result<()> {
        self.inner.load(path)
    }
}
