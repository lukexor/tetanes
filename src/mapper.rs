//! NES Memory Mappers for Cartridges
//!
//! <http://wiki.nesdev.com/w/index.php/Mapper>

use crate::{
    common::{Clock, Kind, NesRegion, Regional, Reset},
    ppu::Mirroring,
};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

pub use m000_nrom::Nrom;
pub use m001_sxrom::{Mmc1Revision, Sxrom};
pub use m002_uxrom::Uxrom;
pub use m003_cnrom::Cnrom;
pub use m004_txrom::{Mmc3Revision, Txrom};
pub use m005_exrom::Exrom;
pub use m007_axrom::Axrom;
pub use m009_pxrom::Pxrom;
pub use m024_m026_vrc6::Vrc6;
pub use m066_gxrom::Gxrom;
pub use m071_bf909x::{Bf909Revision, Bf909x};

pub mod m000_nrom;
pub mod m001_sxrom;
pub mod m002_uxrom;
pub mod m003_cnrom;
pub mod m004_txrom;
pub mod m005_exrom;
pub mod m007_axrom;
pub mod m009_pxrom;
pub mod m024_m026_vrc6;
pub mod m066_gxrom;
pub mod m071_bf909x;
pub mod vrc_irq;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum MapperRevision {
    Mmc1(Mmc1Revision),
    Mmc3(Mmc3Revision),
    Bf909(Bf909Revision),
}

#[enum_dispatch]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
#[must_use]
pub enum Mapper {
    Empty,
    Nrom,
    Sxrom,
    Uxrom,
    Cnrom,
    Txrom,
    Exrom,
    Axrom,
    Pxrom,
    Vrc6,
    Gxrom,
    Bf909x,
}

impl Mapper {
    pub fn none() -> Self {
        Empty.into()
    }
}

impl Default for Mapper {
    fn default() -> Self {
        Self::none()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum MappedRead {
    None,
    Chr(usize),
    CIRam(usize),
    ExRam(usize),
    PrgRom(usize),
    PrgRam(usize),
    Data(u8),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum MappedWrite {
    None,
    Chr(usize, u8),
    CIRam(usize, u8),
    ExRam(usize, u8),
    PrgRam(usize, u8),
    PrgRamProtect(bool),
}

#[enum_dispatch(Mapper)]
pub trait MemMap {
    fn map_read(&mut self, addr: u16) -> MappedRead {
        self.map_peek(addr)
    }

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr.into()),
            0x2000..=0x3EFF => MappedRead::CIRam(addr.into()),
            0x6000..=0x7FFF => MappedRead::PrgRam((addr & 0x1FFF).into()),
            0x8000..=0xFFFF => MappedRead::PrgRom((addr & 0x7FFF).into()),
            _ => MappedRead::None,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(addr.into(), val),
            0x2000..=0x3EFF => MappedWrite::CIRam(addr.into(), val),
            0x6000..=0x7FFF => MappedWrite::PrgRam((addr & 0x1FFF).into(), val),
            _ => MappedWrite::None,
        }
    }
}

#[enum_dispatch(Mapper)]
pub trait Mapped {
    #[must_use]
    fn irq_pending(&self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        Mirroring::default()
    }
    fn set_mirroring(&mut self, _mirroring: Mirroring) {}
    fn ppu_bus_read(&mut self, _addr: u16) {}
    fn ppu_bus_write(&mut self, _addr: u16, _val: u8) {}
    fn cpu_bus_read(&mut self, _addr: u16) {}
    fn cpu_bus_write(&mut self, _addr: u16, _val: u8) {}
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Empty;

impl MemMap for Empty {}
impl Mapped for Empty {}
impl Clock for Empty {}
impl Regional for Empty {}
impl Reset for Empty {}
