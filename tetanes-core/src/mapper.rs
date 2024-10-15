//! Memory Mappers for cartridges.
//!
//! <http://wiki.nesdev.com/w/index.php/Mapper>

use crate::{
    common::{Clock, Regional, Reset, Sram},
    mem,
    ppu::Mirroring,
};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

pub use bandai_fcg::BandaiFCG; // m016, m153, m157, m159
pub use m000_nrom::Nrom;
pub use m001_sxrom::{Revision as Mmc1Revision, Sxrom};
pub use m002_uxrom::Uxrom;
pub use m003_cnrom::Cnrom;
pub use m004_txrom::{Revision as Mmc3Revision, Txrom};
pub use m005_exrom::Exrom;
pub use m007_axrom::Axrom;
pub use m009_pxrom::Pxrom;
pub use m010_fxrom::Fxrom;
pub use m011_color_dreams::ColorDreams;
pub use m024_m026_vrc6::Vrc6;
pub use m034_bnrom::Bnrom;
pub use m034_nina001::Nina001;
pub use m066_gxrom::Gxrom;
pub use m069_sunsoft_fme7::SunsoftFme7;
pub use m071_bf909x::{Bf909x, Revision as Bf909Revision};
pub use m076_dxrom::Dxrom as Dxrom76;
pub use m079_nina003_006::Nina003006;
pub use m088_dxrom::Dxrom as Dxrom88;
pub use m095_dxrom::Dxrom as Dxrom95;
pub use m154_dxrom::Dxrom as Dxrom154;
pub use m206_dxrom::Dxrom as Dxrom206;

pub mod bandai_fcg;
pub mod m000_nrom;
pub mod m001_sxrom;
pub mod m002_uxrom;
pub mod m003_cnrom;
pub mod m004_txrom;
pub mod m005_exrom;
pub mod m007_axrom;
pub mod m009_pxrom;
pub mod m010_fxrom;
pub mod m011_color_dreams;
pub mod m024_m026_vrc6;
pub mod m034_bnrom;
pub mod m034_nina001;
pub mod m066_gxrom;
pub mod m069_sunsoft_fme7;
pub mod m071_bf909x;
pub mod m076_dxrom;
pub mod m079_nina003_006;
pub mod m088_dxrom;
pub mod m095_dxrom;
pub mod m154_dxrom;
pub mod m206_dxrom;
pub mod vrc_irq;

#[derive(thiserror::Error, Debug)]
#[must_use]
pub enum Error {
    #[error(transparent)]
    Bank(#[from] mem::Error),
}

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
    None,
    Nrom,
    Sxrom,
    Uxrom,
    Cnrom,
    Txrom,
    Exrom,
    Axrom,
    Pxrom,
    Fxrom,
    ColorDreams,
    BandaiFCG,
    Vrc6,
    Bnrom,
    Nina001,
    Gxrom,
    SunsoftFme7,
    Bf909x,
    Dxrom76,
    Nina003006,
    Dxrom88,
    Dxrom95,
    Dxrom154,
    Dxrom206,
}

impl Mapper {
    pub fn none() -> Self {
        None.into()
    }

    pub const fn is_none(&self) -> bool {
        matches!(self, Self::None(_))
    }
}

impl Default for Mapper {
    fn default() -> Self {
        Self::none()
    }
}

/// Type of read operation for an address for a given Mapper.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum MappedRead {
    /// Defer to default data bus behavior for this read. Primarily used to read from
    /// a mirrored Console-Internal RAM (i.e Nametable) address.
    Bus,
    /// Read from a CHR ROM or RAM address.
    Chr(usize),
    /// Read from a non-mirrored Console-Internal RAM (i.e. Nameteable) address for Mappers that
    /// support custom Nametable Mirroring.
    CIRam(usize),
    /// Read from an External RAM address for Mappers that support EXRAM.
    ExRam(usize),
    /// Read from a PRG ROM address.
    PrgRom(usize),
    /// Read from a PRG ROM address.
    PrgRam(usize),
    /// Provide data directly for this read (i.e. from an internal Mapper register).
    Data(u8),
}

/// Type of write operation for an address for a given Mapper.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum MappedWrite {
    /// Do nothing with this write.
    None,
    /// Defer to default data bus behavior for this write.
    Bus,
    /// Write value to CHR RAM address.
    ChrRam(usize, u8),
    /// Write value to a non-mirrored Console-Internal RAM (i.e. Nametable) address for Mappers
    /// that support custom Nametable Mirroring.
    CIRam(usize, u8),
    /// Write value to an External RAM address for Mappers that support EXRAM.
    ExRam(usize, u8),
    /// Write value to a PRG RAM address.
    PrgRam(usize, u8),
    /// Toggle PRG RAM write protection.
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
pub struct None;

impl MemMap for None {}
impl Mapped for None {}
impl Clock for None {}
impl Regional for None {}
impl Reset for None {}
impl Sram for None {}
