//! Memory Mappers for cartridges.
//!
//! <https://wiki.nesdev.org/w/index.php/Mapper>

use crate::{
    common::{Clock, Regional, Reset, Sram},
    mem,
    ppu::Mirroring,
};
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
use std::path::Path;

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

/// A `Mapper` is a specific cart variant with dedicated memory mapping logic for memory addressing and
/// bank switching.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub enum Mapper {
    None(()),
    /// `NROM` (Mapper 000)
    Nrom(Nrom),
    /// `SxROM`/`MMC1` (Mapper 001)
    Sxrom(Sxrom),
    /// `UxROM` (Mapper 002)
    Uxrom(Uxrom),
    /// `CNROM` (Mapper 003)
    Cnrom(Cnrom),
    /// `TxROM`/`MMC3` (Mapper 004)
    Txrom(Txrom),
    /// `ExROM`/`MMC5` (Mapper 5)
    Exrom(Exrom),
    /// `AxROM` (Mapper 007)
    Axrom(Axrom),
    /// `PxROM`/`MMC2` (Mapper 009)
    Pxrom(Pxrom),
    /// `FxROM`/`MMC4` (Mapper 010)
    Fxrom(Fxrom),
    /// `Color Dreams` (Mapper 011)
    ColorDreams(ColorDreams),
    /// `Bandai FCG` (Mappers 016, 153, 157, and 159)
    BandaiFCG(BandaiFCG),
    /// `Jaleco SS88006` (Mapper 018)
    JalecoSs88006(JalecoSs88006),
    /// `Namco163` (Mapper 019)
    Namco163(Namco163),
    /// `VRC6` (Mapper 024).
    Vrc6(Vrc6),
    /// `BNROM` (Mapper 034).
    Bnrom(Bnrom),
    /// `NINA-001` (Mapper 034).
    Nina001(Nina001),
    /// `GxROM` (Mapper 066).
    Gxrom(Gxrom),
    /// `Sunsoft FME7` (Mapper 069).
    SunsoftFme7(SunsoftFme7),
    /// `Bf909x` (Mapper 071).
    Bf909x(Bf909x),
    /// `DxROM`/`NAMCOT-3446` (Mapper 076).
    Dxrom76(Dxrom76),
    /// `NINA-003`/`NINA-006` (Mapper 079).
    Nina003006(Nina003006),
    /// `DxROM`/`Namco 108` (Mapper 088).
    Dxrom88(Dxrom88),
    /// `DxROM`/`NAMCOT-3425` (Mapper 095).
    Dxrom95(Dxrom95),
    /// `DxROM`/`NAMCOT-3453` (Mapper 154).
    Dxrom154(Dxrom154),
    /// `DxROM`/`Namco 108` (Mapper 206).
    Dxrom206(Dxrom206),
}

macro_rules! impl_from_board {
    ($($variant:ident($board:ty)),+$(,)?) => (
        $(
            impl From<$board> for Mapper {
                fn from(board: $board) -> Self {
                    Self::$variant(board)
                }
            }
        )+
    )
}

impl_from_board!(
    Nrom(Nrom),
    Sxrom(Sxrom),
    Uxrom(Uxrom),
    Cnrom(Cnrom),
    Txrom(Txrom),
    Exrom(Exrom),
    Axrom(Axrom),
    Pxrom(Pxrom),
    Fxrom(Fxrom),
    ColorDreams(ColorDreams),
    BandaiFCG(BandaiFCG),
    JalecoSs88006(JalecoSs88006),
    Namco163(Namco163),
    Vrc6(Vrc6),
    Bnrom(Bnrom),
    Nina001(Nina001),
    Gxrom(Gxrom),
    SunsoftFme7(SunsoftFme7),
    Bf909x(Bf909x),
    Dxrom76(Dxrom76),
    Nina003006(Nina003006),
    Dxrom88(Dxrom88),
    Dxrom95(Dxrom95),
    Dxrom154(Dxrom154),
    Dxrom206(Dxrom206),
);

macro_rules! impl_map {
    ($self:expr, $fn:ident$(,)? $($args:expr),*$(,)?) => {
        match $self {
            Mapper::None(m) => m.$fn($($args),*),
            Mapper::Nrom(m) => m.$fn($($args),*),
            Mapper::Sxrom(m) => m.$fn($($args),*),
            Mapper::Uxrom(m) => m.$fn($($args),*),
            Mapper::Cnrom(m) => m.$fn($($args),*),
            Mapper::Txrom(m) => m.$fn($($args),*),
            Mapper::Exrom(m) => m.$fn($($args),*),
            Mapper::Axrom(m) => m.$fn($($args),*),
            Mapper::Pxrom(m) => m.$fn($($args),*),
            Mapper::Fxrom(m) => m.$fn($($args),*),
            Mapper::ColorDreams(m) => m.$fn($($args),*),
            Mapper::BandaiFCG(m) => m.$fn($($args),*),
            Mapper::JalecoSs88006(m) => m.$fn($($args),*),
            Mapper::Namco163(m) => m.$fn($($args),*),
            Mapper::Vrc6(m) => m.$fn($($args),*),
            Mapper::Bnrom(m) => m.$fn($($args),*),
            Mapper::Nina001(m) => m.$fn($($args),*),
            Mapper::Gxrom(m) => m.$fn($($args),*),
            Mapper::SunsoftFme7(m) => m.$fn($($args),*),
            Mapper::Bf909x(m) => m.$fn($($args),*),
            Mapper::Dxrom76(m) => m.$fn($($args),*),
            Mapper::Nina003006(m) => m.$fn($($args),*),
            Mapper::Dxrom88(m) => m.$fn($($args),*),
            Mapper::Dxrom95(m) => m.$fn($($args),*),
            Mapper::Dxrom154(m) => m.$fn($($args),*),
            Mapper::Dxrom206(m) => m.$fn($($args),*),
        }
    };
}

impl Map for Mapper {
    fn map_read(&mut self, addr: u16) -> MappedRead {
        impl_map!(self, map_read, addr)
    }

    fn map_peek(&self, addr: u16) -> MappedRead {
        impl_map!(self, map_peek, addr)
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        impl_map!(self, map_write, addr, val)
    }

    fn bus_read(&mut self, addr: u16, kind: BusKind) {
        impl_map!(self, bus_read, addr, kind)
    }

    fn bus_write(&mut self, addr: u16, val: u8, kind: BusKind) {
        impl_map!(self, bus_write, addr, val, kind)
    }

    fn mirroring(&self) -> Mirroring {
        impl_map!(self, mirroring)
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        impl_map!(self, set_mirroring, mirroring)
    }
}

impl Reset for Mapper {
    fn reset(&mut self, kind: crate::prelude::ResetKind) {
        impl_map!(self, reset, kind)
    }
}

impl Clock for Mapper {
    fn clock(&mut self) {
        impl_map!(self, clock)
    }
}

impl Regional for Mapper {
    fn region(&self) -> crate::prelude::NesRegion {
        impl_map!(self, region)
    }

    fn set_region(&mut self, region: crate::prelude::NesRegion) {
        impl_map!(self, set_region, region)
    }
}

impl Sram for Mapper {
    fn save(&self, path: impl AsRef<Path>) -> crate::fs::Result<()> {
        impl_map!(self, save, path)
    }

    fn load(&mut self, path: impl AsRef<Path>) -> crate::fs::Result<()> {
        impl_map!(self, load, path)
    }
}

impl Mapper {
    pub const fn none() -> Self {
        Self::None(())
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
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum MappedRead {
    /// Defer to default data bus behavior for this read. Primarily used to read from
    /// a mirrored Console-Internal RAM (i.e Nametable) address.
    #[default]
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
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum MappedWrite {
    /// Do nothing with this write.
    None,
    /// Defer to default data bus behavior for this write.
    #[default]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum BusKind {
    Cpu,
    Ppu,
}

/// Trait implemented for all [`Mapper`]s.
pub trait Map: Clock + Regional + Reset + Sram {
    /// Determine the [`MappedRead`] for the given address.
    fn map_read(&mut self, addr: u16) -> MappedRead {
        self.map_peek(addr)
    }

    /// Determine the [`MappedRead`] for the given address, but do not modify any internal state.
    fn map_peek(&self, _addr: u16) -> MappedRead {
        MappedRead::default()
    }

    /// Determine the [`MappedWrite`] for the given address and value.
    fn map_write(&mut self, _addr: u16, _val: u8) -> MappedWrite {
        MappedWrite::default()
    }

    /// Simulates a read for the given bus at the given address for mappers that use bus reads for
    /// timing.
    fn bus_read(&mut self, _addr: u16, _kind: BusKind) {}

    /// Simulates a write for the given bus at the given address for mappers that use bus writes for
    /// timing.
    fn bus_write(&mut self, _addr: u16, _val: u8, _kind: BusKind) {}

    /// Returns the current [`Mirroring`] mode.
    fn mirroring(&self) -> Mirroring {
        Mirroring::default()
    }

    /// Set the [`Mirroring`] mode.
    fn set_mirroring(&mut self, _mirroring: Mirroring) {}
}

impl Map for () {
    fn map_read(&mut self, addr: u16) -> MappedRead {
        self.map_peek(addr)
    }

    fn map_peek(&self, _addr: u16) -> MappedRead {
        MappedRead::default()
    }

    fn map_write(&mut self, _addr: u16, _val: u8) -> MappedWrite {
        MappedWrite::default()
    }

    fn bus_read(&mut self, _addr: u16, _kind: BusKind) {}

    fn bus_write(&mut self, _addr: u16, _val: u8, _kind: BusKind) {}

    fn mirroring(&self) -> Mirroring {
        Mirroring::default()
    }

    fn set_mirroring(&mut self, _mirroring: Mirroring) {}
}

impl Reset for () {}
impl Clock for () {}
impl Regional for () {}
impl Sram for () {}
