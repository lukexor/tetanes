//! `DxROM`/`NAMCOT-3453` (Mapper 154)
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_154>
//! <https://www.nesdev.org/wiki/DxROM>

use crate::{
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sram},
    fs,
    mapper::{self, Dxrom88, Mapped, MappedRead, MappedWrite, Mapper, MemMap},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Dxrom {
    pub inner: Dxrom88,
}

impl Dxrom {
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        Ok(Dxrom88::new(cart, Dxrom88::CHR_WINDOW)?.into())
    }
}

impl Mapped for Dxrom {
    fn mirroring(&self) -> Mirroring {
        self.inner.mirroring()
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.inner.set_mirroring(mirroring);
    }

    fn ppu_bus_read(&mut self, addr: u16) {
        self.inner.ppu_bus_read(addr)
    }

    fn ppu_bus_write(&mut self, addr: u16, val: u8) {
        self.inner.ppu_bus_write(addr, val)
    }

    fn cpu_bus_read(&mut self, addr: u16) {
        self.inner.cpu_bus_read(addr)
    }

    fn cpu_bus_write(&mut self, addr: u16, val: u8) {
        self.inner.cpu_bus_write(addr, val)
    }
}

impl MemMap for Dxrom {
    fn map_read(&mut self, addr: u16) -> MappedRead {
        self.inner.map_read(addr)
    }

    fn map_peek(&self, addr: u16) -> MappedRead {
        self.inner.map_peek(addr)
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        self.set_mirroring(if val & 0x40 == 0x40 {
            Mirroring::SingleScreenB
        } else {
            Mirroring::SingleScreenA
        });
        self.inner.map_write(addr, val)
    }
}

impl Reset for Dxrom {
    fn reset(&mut self, kind: ResetKind) {
        self.inner.reset(kind);
    }
}
impl Clock for Dxrom {
    fn clock(&mut self) -> usize {
        self.inner.clock()
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
