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
pub use m018_jalecoss88006::JalecoSs88006;
pub use m019_namco163::Namco163;
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
pub mod m018_jalecoss88006;
pub mod m019_namco163;
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

/// Errors that mappers can return.
#[derive(thiserror::Error, Debug)]
#[must_use]
pub enum Error {
    /// A mapper banking error.
    #[error(transparent)]
    Bank(#[from] mem::Error),
}

/// Allow user-controlled mapper revision for mappers that are difficult to auto-detect correctly.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum MapperRevision {
    // Mmc1 and Vrc6 should be properly detected by the mapper number
    /// No known detection except DB lookup
    Mmc3(Mmc3Revision),
    /// Can compare to submapper 1, if header is correct
    Bf909(Bf909Revision),
}

impl std::fmt::Display for MapperRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Mmc3(rev) => match rev {
                Mmc3Revision::A => "MMC3A",
                Mmc3Revision::BC => "MMC3B/C",
                Mmc3Revision::Acc => "MMC3Acc",
            },
            Self::Bf909(rev) => match rev {
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
    /// `NROM` (Mapper 000)
    Nrom,
    /// `SxROM`/`MMC1` (Mapper 001)
    Sxrom,
    /// `UxROM` (Mapper 002)
    Uxrom,
    /// `CNROM` (Mapper 003)
    Cnrom,
    /// `TxROM`/`MMC3` (Mapper 004)
    Txrom,
    /// `ExROM`/`MMC5` (Mapper 5)
    Exrom,
    /// `AxROM` (Mapper 007)
    Axrom,
    /// `PxROM`/`MMC2` (Mapper 009)
    Pxrom,
    /// `FxROM`/`MMC4` (Mapper 010)
    Fxrom,
    /// `Color Dreams` (Mapper 011)
    ColorDreams,
    /// `Bandai FCG` (Mappers 016, 153, 157, and 159)
    BandaiFCG,
    /// `Jaleco SS88006` (Mapper 018)
    JalecoSs88006,
    /// `Namco163` (Mapper 019)
    Namco163,
    /// `VRC6` (Mapper 024).
    Vrc6,
    /// `BNROM` (Mapper 034).
    Bnrom,
    /// `NINA-001` (Mapper 034).
    Nina001,
    /// `GxROM` (Mapper 066).
    Gxrom,
    /// `Sunsoft FME7` (Mapper 069).
    SunsoftFme7,
    /// `Bf909x` (Mapper 071).
    Bf909x,
    /// `DxROM`/`NAMCOT-3446` (Mapper 076).
    Dxrom76,
    /// `NINA-003`/`NINA-006` (Mapper 079).
    Nina003006,
    /// `DxROM`/`Namco 108` (Mapper 088).
    Dxrom88,
    /// `DxROM`/`NAMCOT-3425` (Mapper 095).
    Dxrom95,
    /// `DxROM`/`NAMCOT-3453` (Mapper 154).
    Dxrom154,
    /// `DxROM`/`Namco 108` (Mapper 206).
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
pub trait MapRead {
    fn map_read(&mut self, addr: u16) -> MappedRead {
        self.map_peek(addr)
    }

    fn map_peek(&self, _addr: u16) -> MappedRead {
        MappedRead::Bus
    }
}

#[enum_dispatch(Mapper)]
pub trait MapWrite {
    fn map_write(&mut self, _addr: u16, _val: u8) -> MappedWrite {
        MappedWrite::Bus
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum BusKind {
    Cpu,
    Ppu,
}

#[enum_dispatch(Mapper)]
pub trait OnBusRead {
    fn on_bus_read(&mut self, _addr: u16, _kind: BusKind) {}
}

#[enum_dispatch(Mapper)]
pub trait OnBusWrite {
    fn on_bus_write(&mut self, _addr: u16, _val: u8, _kind: BusKind) {}
}

#[enum_dispatch(Mapper)]
pub trait Mirrored {
    fn mirroring(&self) -> Mirroring {
        Mirroring::default()
    }
    fn set_mirroring(&mut self, _mirroring: Mirroring) {}
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct None;

impl MapRead for None {}
impl MapWrite for None {}
impl OnBusRead for None {}
impl OnBusWrite for None {}
impl Mirrored for None {}
impl Clock for None {}
impl Regional for None {}
impl Reset for None {}
impl Sram for None {}
