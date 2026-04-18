//! Memory Mappers for cartridges.
//!
//! <https://wiki.nesdev.org/w/index.php/Mapper>

use crate::{
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sample, Sram},
    fs, mem,
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
pub use m079_nina003_006::Nina003006;

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
pub mod m079_nina003_006;
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
    /// `TxROM`/`MMC3` (Mappers 004, 088, 095, 206)
    Txrom(Txrom),
    /// `ExROM`/`MMC5` (Mapper 5)
    Exrom(Box<Exrom>),
    /// `AxROM` (Mapper 007)
    Axrom(Axrom),
    /// `PxROM`/`MMC2` (Mapper 009)
    Pxrom(Pxrom),
    /// `FxROM`/`MMC4` (Mapper 010)
    Fxrom(Fxrom),
    /// `Color Dreams` (Mapper 011)
    ColorDreams(ColorDreams),
    /// `Bandai FCG` (Mappers 016, 153, 157, and 159)
    BandaiFCG(Box<BandaiFCG>),
    /// `Jaleco SS88006` (Mapper 018)
    JalecoSs88006(JalecoSs88006),
    /// `Namco163` (Mapper 019)
    Namco163(Box<Namco163>),
    /// `VRC6` (Mapper 024).
    Vrc6(Box<Vrc6>),
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
    /// `NINA-003`/`NINA-006` (Mapper 079).
    Nina003006(Nina003006),
}

/// Implement `From<T>` for `Mapper`.
macro_rules! impl_from_board {
    (@impl $variant:ident, $board:ident) => {
        impl From<$board> for Mapper {
            fn from(board: $board) -> Self {
                Self::$variant(board)
            }
        }
    };
    (@impl $variant:ident, Box<$board:ident>) => {
        impl From<$board> for Mapper {
            fn from(board: $board) -> Self {
                Self::$variant(Box::new(board))
            }
        }
        impl From<Box<$board>> for Mapper {
            fn from(board: Box<$board>) -> Self {
                Self::$variant(board)
            }
        }
    };
    ($($variant:ident($($tt:tt)+)),+ $(,)?) => {
        $(impl_from_board!(@impl $variant, $($tt)+);)+
    };
}

impl_from_board!(
    Nrom(Nrom),
    Sxrom(Sxrom),
    Uxrom(Uxrom),
    Cnrom(Cnrom),
    Txrom(Txrom),
    Exrom(Box<Exrom>),
    Axrom(Axrom),
    Pxrom(Pxrom),
    Fxrom(Fxrom),
    ColorDreams(ColorDreams),
    BandaiFCG(Box<BandaiFCG>),
    JalecoSs88006(JalecoSs88006),
    Namco163(Box<Namco163>),
    Vrc6(Box<Vrc6>),
    Bnrom(Bnrom),
    Nina001(Nina001),
    Gxrom(Gxrom),
    SunsoftFme7(SunsoftFme7),
    Bf909x(Bf909x),
    Nina003006(Nina003006),
);

/// Implement `Map` function for all `Mapper` variants.
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
            Mapper::Nina003006(m) => m.$fn($($args),*),
        }
    };
}

impl Map for Mapper {
    /// Read a byte from CHR-ROM/RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_read(&mut self, addr: u16, ciram: &CIRam) -> u8 {
        impl_map!(self, chr_read, addr, ciram)
    }

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        impl_map!(self, chr_peek, addr, ciram)
    }

    /// Read a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_read(&mut self, addr: u16) -> u8 {
        impl_map!(self, prg_read, addr)
    }

    /// Read a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_peek(&self, addr: u16) -> u8 {
        impl_map!(self, prg_peek, addr)
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        impl_map!(self, chr_write, addr, val, ciram)
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        impl_map!(self, prg_write, addr, val)
    }

    /// Synchronize a read from a PPU address.
    fn ppu_read(&mut self, addr: u16) {
        impl_map!(self, ppu_read, addr)
    }

    /// Synchronize a write to a PPU address.
    fn ppu_write(&mut self, addr: u16, val: u8) {
        impl_map!(self, ppu_write, addr, val)
    }

    /// Whether an IRQ is pending acknowledgement.
    fn irq_pending(&self) -> bool {
        impl_map!(self, irq_pending)
    }

    /// Whether an DMA is pending acknowledgement.
    fn dma_pending(&self) -> bool {
        impl_map!(self, dma_pending)
    }

    /// Clear pending DMA.
    fn clear_dma_pending(&mut self) {
        impl_map!(self, clear_dma_pending)
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        impl_map!(self, mirroring)
    }
}

impl Sample for Mapper {
    /// Output a single audio sample.
    #[inline]
    fn output(&self) -> f32 {
        match self {
            Self::Exrom(exrom) => exrom.output(),
            Self::Namco163(namco163) => namco163.output(),
            Self::Vrc6(vrc6) => vrc6.output(),
            Self::SunsoftFme7(sunsoft_fme7) => sunsoft_fme7.output(),
            _ => 0.0,
        }
    }
}

impl Reset for Mapper {
    /// Reset the component given the [`ResetKind`].
    fn reset(&mut self, kind: ResetKind) {
        impl_map!(self, reset, kind)
    }
}

impl Clock for Mapper {
    /// Clock component once.
    #[inline]
    fn clock(&mut self) {
        impl_map!(self, clock)
    }
}

impl Regional for Mapper {
    /// Return the current region.
    fn region(&self) -> NesRegion {
        impl_map!(self, region)
    }

    /// Set the region.
    fn set_region(&mut self, region: NesRegion) {
        impl_map!(self, set_region, region)
    }
}

impl Sram for Mapper {
    /// Save RAM to a given path.
    fn save(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        impl_map!(self, save, path)
    }

    /// Load save RAM from a given path.
    fn load(&mut self, path: impl AsRef<Path>) -> fs::Result<()> {
        impl_map!(self, load, path)
    }
}

impl Mapper {
    /// An empty Mapper.
    pub const fn none() -> Self {
        Self::None(())
    }

    /// Whether mapper is `None`.
    pub const fn is_none(&self) -> bool {
        matches!(self, Self::None(_))
    }
}

impl Default for Mapper {
    fn default() -> Self {
        Self::none()
    }
}

/// Trait implemented for all [`Mapper`]s.
pub trait Map: Clock + Regional + Reset + Sram {
    /// Read a byte from CHR-ROM/RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_read(&mut self, addr: u16, ciram: &CIRam) -> u8 {
        self.chr_peek(addr, ciram)
    }

    /// Peek a byte from CHR-ROM/RAM at a given address.
    // `chr_peek` has to be implemented at read from CHR and CIRam.
    fn chr_peek(&self, _addr: u16, _ciram: &CIRam) -> u8;

    /// Read a byte from PRG-ROM/RAM at a given address.
    ///
    /// Defaults to `prg_peek`.
    #[inline(always)]
    fn prg_read(&mut self, addr: u16) -> u8 {
        self.prg_peek(addr)
    }

    /// Peek a byte from PRG-ROM/RAM at a given address.
    // `prg_peek` has to be implemented to read PRG-ROM.
    fn prg_peek(&self, _addr: u16) -> u8;

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    // `chr_write` has to be implemented at least to write to CIRam.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        if let 0x2000..=0x3EFF = addr {
            ciram.write(addr, val, self.mirroring());
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    fn prg_write(&mut self, _addr: u16, _val: u8) {}

    /// Synchronize a read from a PPU address.
    fn ppu_read(&mut self, _addr: u16) {}

    /// Synchronize a write to a PPU address.
    fn ppu_write(&mut self, _addr: u16, _val: u8) {}

    /// Whether an IRQ is pending acknowledgement.
    fn irq_pending(&self) -> bool {
        false
    }

    /// Clear pending DMA.
    fn clear_dma_pending(&mut self) {}

    /// Whether an DMA is pending acknowledgement.
    fn dma_pending(&self) -> bool {
        false
    }

    /// Returns the current [`Mirroring`] mode.
    // All mappers have mirroring, even if it's hard-wired.
    fn mirroring(&self) -> Mirroring;
}

impl Map for () {
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x2000..=0x3EFF => ciram.peek(addr, self.mirroring()),
            _ => 0,
        }
    }

    fn prg_peek(&self, _addr: u16) -> u8 {
        0
    }

    fn mirroring(&self) -> Mirroring {
        Mirroring::default()
    }
}

impl Sample for () {}
impl Reset for () {}
impl Clock for () {}
impl Regional for () {}
impl Sram for () {}
