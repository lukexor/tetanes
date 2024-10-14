//! `DxROM`/`Namco 108` (Mapper 206)
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_206>
//! <https://www.nesdev.org/wiki/DxROM>

use crate::{
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sram},
    fs,
    mapper::{self, Mapped, MappedRead, MappedWrite, Mapper, MemMap, Txrom},
    mem::Banks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Dxrom {
    pub inner: Txrom,
}

impl Dxrom {
    pub const CHR_WINDOW: usize = Txrom::CHR_WINDOW;

    pub fn new(cart: &mut Cart, chr_window: usize) -> Result<Self, mapper::Error> {
        Ok(Self {
            inner: Txrom::new(cart, chr_window)?,
        })
    }

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        Ok(Self::new(cart, Self::CHR_WINDOW)?.into())
    }

    pub const fn bank_register(&self, index: usize) -> u8 {
        self.inner.bank_register(index)
    }

    pub fn set_chr_banks(&mut self, f: impl Fn(&mut Banks, &mut [u8])) {
        self.inner.set_chr_banks(f);
    }

    pub fn update_chr_banks(&mut self) {
        self.inner.update_chr_banks();
    }
}

impl Mapped for Dxrom {
    fn mirroring(&self) -> Mirroring {
        self.inner.mirroring()
    }

    fn set_mirroring(&mut self, _mirroring: Mirroring) {
        // Mirroring is hardwired
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

    fn map_write(&mut self, mut addr: u16, mut val: u8) -> MappedWrite {
        // Redirect all 0x8000..=0xFFFF writes to 0x8000..=0x8001
        if matches!(addr, 0x8000..=0xFFFF) {
            addr &= 0x8001;
            if addr == 0x8000 {
                // Disable CHR mode 1 and Prg mode 1
                val &= 0x3F;
            }
        }
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
