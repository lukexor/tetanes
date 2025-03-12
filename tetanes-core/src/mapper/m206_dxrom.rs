//! `DxROM`/`Namco 108` (Mapper 206).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_206>
//! <https://www.nesdev.org/wiki/DxROM>

use crate::{
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sram},
    fs,
    mapper::{
        self, BusKind, MapRead, MapWrite, MappedRead, MappedWrite, Mapper, Mirrored, OnBusRead,
        OnBusWrite, Txrom,
    },
    mem::Banks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `DxROM`/`Namco 108` (Mapper 206).
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

impl Mirrored for Dxrom {
    fn mirroring(&self) -> Mirroring {
        self.inner.mirroring()
    }

    fn set_mirroring(&mut self, _mirroring: Mirroring) {
        // Mirroring is hardwired
    }
}

impl OnBusRead for Dxrom {
    fn on_bus_read(&mut self, addr: u16, kind: BusKind) {
        self.inner.on_bus_read(addr, kind)
    }
}

impl OnBusWrite for Dxrom {
    fn on_bus_write(&mut self, addr: u16, val: u8, kind: BusKind) {
        self.inner.on_bus_write(addr, val, kind)
    }
}

impl MapRead for Dxrom {
    // PPU $0000..=$07FF (or $1000..=$17FF) 2K CHR-ROM/RAM Bank 1 Switchable --+
    // PPU $0800..=$0FFF (or $1800..=$1FFF) 2K CHR-ROM/RAM Bank 2 Switchable --|-+
    // PPU $1000..=$13FF (or $0000..=$03FF) 1K CHR-ROM/RAM Bank 3 Switchable --+ |
    // PPU $1400..=$17FF (or $0400..=$07FF) 1K CHR-ROM/RAM Bank 4 Switchable --+ |
    // PPU $1800..=$1BFF (or $0800..=$0BFF) 1K CHR-ROM/RAM Bank 5 Switchable ----+
    // PPU $1C00..=$1FFF (or $0C00..=$0FFF) 1K CHR-ROM/RAM Bank 6 Switchable ----+

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
}

impl MapWrite for Dxrom {
    fn map_write(&mut self, mut addr: u16, mut val: u8) -> MappedWrite {
        // Apply register mask
        addr &= 0xE001;
        if addr == 0x8000 {
            // Disable CHR mode 1 and Prg mode 1
            val &= 0x3F;
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
    fn clock(&mut self) -> u64 {
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
