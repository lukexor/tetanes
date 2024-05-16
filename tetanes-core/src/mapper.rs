//! Memory Mappers for cartridges.
//!
//! <http://wiki.nesdev.com/w/index.php/Mapper>

use crate::{
    common::{Clock, NesRegion, Regional, Reset, ResetKind},
    ppu::Mirroring,
};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

pub use m000_nrom::Nrom;
pub use m001_sxrom::{Revision as Mmc1Revision, Sxrom};
pub use m002_uxrom::Uxrom;
pub use m003_cnrom::Cnrom;
pub use m004_txrom::{Revision as Mmc3Revision, Txrom};
pub use m005_exrom::Exrom;
pub use m007_axrom::Axrom;
pub use m009_pxrom::Pxrom;
pub use m024_m026_vrc6::Vrc6;
pub use m034_bnrom::Bnrom;
pub use m034_nina001::Nina001;
pub use m066_gxrom::Gxrom;
pub use m071_bf909x::{Bf909x, Revision as Bf909Revision};

pub mod m000_nrom;
pub mod m001_sxrom;
pub mod m002_uxrom;
pub mod m003_cnrom;
pub mod m004_txrom;
pub mod m005_exrom;
pub mod m007_axrom;
pub mod m009_pxrom;
pub mod m024_m026_vrc6;
pub mod m034_bnrom;
pub mod m034_nina001;
pub mod m066_gxrom;
pub mod m071_bf909x;
pub mod vrc_irq;

/// Allow user-controlled mapper revision for mappers that are difficult to auto-detect correctly.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum MapperRevision {
    // Mmc1 and Vrc6 should be properly detected by the mapper number
    Mmc3(Mmc3Revision),   // No known detection except DB lookup
    Bf909(Bf909Revision), // Can compare to submapper 1, if header is correct
}

impl std::fmt::Display for MapperRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MapperRevision::Mmc3(rev) => match rev {
                Mmc3Revision::A => "MMC3A",
                Mmc3Revision::BC => "MMC3B/C",
                Mmc3Revision::Acc => "MMC3Acc",
            },
            MapperRevision::Bf909(rev) => match rev {
                Bf909Revision::Bf909x => "BF909x",
                Bf909Revision::Bf9097 => "BF9097",
            },
        };
        write!(f, "{s}")
    }
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
    Bnrom,
    Nina001,
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
    Bus,
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
    Bus,
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

    fn map_peek(&self, _addr: u16) -> MappedRead {
        MappedRead::Bus
    }

    fn map_write(&mut self, _addr: u16, _val: u8) -> MappedWrite {
        MappedWrite::Bus
    }
}

#[enum_dispatch(Mapper)]
pub trait Mapped {
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
