//! Memory Mappers for cartridges.
//!
//! <https://wiki.nesdev.org/w/index.php/Mapper>

use crate::{
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sample, Sram},
    fs, mem,
    memory::{Memory, Src},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use bandai_fcg::BandaiFCG; // m016, m153, m157, m159
pub use m000_nrom::Nrom;
pub use m001_sxrom::Sxrom;
pub use m002_uxrom::Uxrom;
pub use m003_cnrom::Cnrom;
pub use m004_txrom::Txrom;
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
pub use m105_nes_event::NesEvent;
pub use m176_fk23c::Fk23C;
pub use mmc1::{Mmc1, Revision as Mmc1Revision};
pub use mmc3::{Mmc3, Revision as Mmc3Revision};

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
pub mod m105_nes_event;
pub mod m176_fk23c;
pub mod mmc1;
pub mod mmc3;
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
    /// `NES-EVENT` (Mapper 105)
    NesEvent(NesEvent),
    /// `Waixing FK23C`/`FS303` (Mapper 176)
    // Boxed: at 280 bytes it is now several times the size of any other variant, since the ported
    // boards hold only registers. The remaining unported boards will shrink the same way.
    Fk23C(Box<Fk23C>),
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
    NesEvent(NesEvent),
    Fk23C(Box<Fk23C>),
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
            Mapper::NesEvent(m) => m.$fn($($args),*),
            Mapper::Fk23C(m) => m.$fn($($args),*),
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

    /// Whether this board has been ported to page-table [`Memory`].
    #[inline(always)]
    fn uses_page_tables(&self) -> bool {
        impl_map!(self, uses_page_tables)
    }

    /// Handle a CPU-space write for a page-table board, re-banking as needed.
    #[inline(always)]
    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        impl_map!(self, write_register, memory, addr, val)
    }

    /// Write this board's battery-backed state.
    fn save_sram(&self, memory: &Memory, path: &Path) -> fs::Result<()> {
        impl_map!(self, save_sram, memory, path)
    }

    /// Restore state previously written by `save_sram`.
    fn load_sram(&mut self, memory: &mut Memory, path: &Path) -> fs::Result<()> {
        impl_map!(self, load_sram, memory, path)
    }

    /// Whether this board serves some CPU reads itself.
    #[inline(always)]
    fn has_prg_read_hook(&self) -> bool {
        impl_map!(self, has_prg_read_hook)
    }

    /// Serve a CPU read, returning `None` to fall through to page-table memory.
    #[inline(always)]
    fn prg_read_hook(&mut self, addr: u16) -> Option<u8> {
        impl_map!(self, prg_read_hook, addr)
    }

    /// Side-effect-free form of [`Map::prg_read_hook`].
    #[inline(always)]
    fn prg_peek_hook(&self, addr: u16) -> Option<u8> {
        impl_map!(self, prg_peek_hook, addr)
    }

    /// Whether this board serves some PPU reads itself.
    #[inline(always)]
    fn has_chr_read_hook(&self) -> bool {
        impl_map!(self, has_chr_read_hook)
    }

    /// Serve a PPU read, returning `None` to fall through to page-table memory.
    #[inline(always)]
    fn chr_read_hook(&mut self, memory: &mut Memory, addr: u16) -> Option<u8> {
        impl_map!(self, chr_read_hook, memory, addr)
    }

    /// Side-effect-free form of `chr_read_hook`.
    #[inline(always)]
    fn chr_peek_hook(&self, memory: &Memory, addr: u16) -> Option<u8> {
        impl_map!(self, chr_peek_hook, memory, addr)
    }

    /// Whether this board must observe every PPU bus address.
    #[inline(always)]
    fn watches_ppu_bus(&self) -> bool {
        impl_map!(self, watches_ppu_bus)
    }

    /// Observe a PPU bus address.
    #[inline(always)]
    fn ppu_bus_addr(&mut self, memory: &mut Memory, addr: u16) {
        impl_map!(self, ppu_bus_addr, memory, addr)
    }

    /// Rebuild the page tables from this board's register state.
    fn sync(&mut self, memory: &mut Memory) {
        impl_map!(self, sync, memory)
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
    ///
    /// Boards serving reads from page-table [`Memory`] never see this; routing is the caller's
    /// job, so the default is inert rather than a panic.
    fn chr_peek(&self, _addr: u16, _ciram: &CIRam) -> u8 {
        0
    }

    /// Read a byte from PRG-ROM/RAM at a given address.
    ///
    /// Defaults to `prg_peek`.
    #[inline(always)]
    fn prg_read(&mut self, addr: u16) -> u8 {
        self.prg_peek(addr)
    }

    /// Peek a byte from PRG-ROM/RAM at a given address.
    ///
    /// Boards serving reads from page-table [`Memory`] never see this.
    fn prg_peek(&self, _addr: u16) -> u8 {
        0
    }

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

    /// Whether this board has been ported to page-table [`Memory`].
    ///
    /// Boards are ported a tier at a time, so both paths coexist. A board returning `true` owns no
    /// memory of its own: its reads are served directly from [`Memory`] and `chr_peek`/`prg_peek`
    /// are never called on it. Once every board is ported this, and the read methods above, go
    /// away.
    fn uses_page_tables(&self) -> bool {
        false
    }

    /// Handle a CPU-space write for a page-table board, re-banking as needed.
    ///
    /// Called for every write in `$4020..=$FFFF`; the plain data store into PRG-RAM has already
    /// happened, so this only needs to handle registers.
    fn write_register(&mut self, _memory: &mut Memory, _addr: u16, _val: u8) {}

    /// Write this board's battery-backed state.
    ///
    /// The default saves PRG-RAM, which is what almost every board wants. Boards whose battery
    /// covers something else - Namco163 also keeps internal sound RAM, Bandai's Datach carts have
    /// EEPROMs and no PRG-RAM at all - override this and keep their own on-disk layout, so `Bus`
    /// needs no per-board knowledge.
    fn save_sram(&self, memory: &Memory, path: &Path) -> fs::Result<()> {
        fs::save(path, &memory.region_ref(Src::PrgRam).to_vec())
    }

    /// Restore state previously written by [`Map::save_sram`].
    fn load_sram(&mut self, memory: &mut Memory, path: &Path) -> fs::Result<()> {
        let data = fs::load::<Vec<u8>>(path)?;
        let ram = memory.region_mut(Src::PrgRam);
        let len = ram.len().min(data.len());
        ram[..len].copy_from_slice(&data[..len]);
        Ok(())
    }

    /// Whether this board serves some CPU reads itself rather than from page tables.
    ///
    /// Expansion hardware - Namco163's audio registers and IRQ counter, Bandai's EEPROM and
    /// barcode reader - is not memory and cannot be expressed as a page. Cached at load so boards
    /// without it pay a bool test rather than a call on every PRG read.
    fn has_prg_read_hook(&self) -> bool {
        false
    }

    /// Serve a CPU read, returning `None` to fall through to page-table memory.
    fn prg_read_hook(&mut self, _addr: u16) -> Option<u8> {
        None
    }

    /// Side-effect-free form of [`Map::prg_read_hook`], for debuggers.
    fn prg_peek_hook(&self, _addr: u16) -> Option<u8> {
        None
    }

    /// Whether this board serves some PPU reads itself rather than from page tables.
    ///
    /// MMC5 is the only board that needs this: in extended-attribute mode the CHR bank for a tile
    /// comes from a byte of ExRAM looked up per tile, and attribute and fill-mode reads are
    /// synthesised rather than fetched, so neither is expressible as a page entry. Cached at load
    /// so every other board pays a bool test on the ~41,000 CHR fetches in a frame.
    fn has_chr_read_hook(&self) -> bool {
        false
    }

    /// Serve a PPU read, returning `None` to fall through to page-table memory.
    ///
    /// Runs *before* the page-table read, so a board may also re-bank here: MMC5 swaps its sprite
    /// and background CHR bank sets partway through a scanline, and the swap has to apply to the
    /// fetch that triggered it.
    fn chr_read_hook(&mut self, _memory: &mut Memory, _addr: u16) -> Option<u8> {
        None
    }

    /// Side-effect-free form of [`Map::chr_read_hook`], for debuggers.
    fn chr_peek_hook(&self, _memory: &Memory, _addr: u16) -> Option<u8> {
        None
    }

    /// Whether this board must observe every PPU bus address.
    ///
    /// Boards on page tables no longer see reads, so the A12 rising-edge scanline counters
    /// (MMC3, FK23C) and CHR latches (MMC2, MMC4) need the PPU to notify them explicitly. Cached
    /// at load so the ~20 boards that do not care pay a bool test rather than a dispatch on every
    /// one of the ~41,000 CHR fetches in a frame.
    fn watches_ppu_bus(&self) -> bool {
        false
    }

    /// Observe a PPU bus address, for boards that returned `true` from `watches_ppu_bus`.
    fn ppu_bus_addr(&mut self, _memory: &mut Memory, _addr: u16) {}

    /// Rebuild the page tables from this board's register state.
    ///
    /// Page tables are derived state and are not serialized, so they must be reconstructed after
    /// loading a save state; a board's registers survive, the mapping does not. Also called after
    /// reset, where `Reset` has no access to [`Memory`]. Boards implement their initial mapping
    /// here and call it from `load`, so a fresh cart and a restored save state take the same path.
    fn sync(&mut self, _memory: &mut Memory) {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cart::Cart, memory::Src};

    /// Page tables are `#[serde(skip)]` derived state, and `Cpu::load` replaces the whole console
    /// when a save state is loaded. Without a `sync` on the way back in, every page comes back
    /// unmapped and a restored state reads zeroes.
    #[test]
    fn sync_rebuilds_page_tables_after_a_save_state_round_trip() {
        let mut cart = Cart::empty_sized(0x8000, 0x2000);
        for (i, page) in cart
            .memory
            .region_mut(Src::PrgRom)
            .chunks_mut(1024)
            .enumerate()
        {
            page.fill(i as u8);
        }
        let mut mapper = Uxrom::load(&mut cart).expect("valid mapper");

        // Switch $8000 to bank 1, which starts 16 KiB in.
        mapper.write_register(&mut cart.memory, 0x8000, 1);
        assert_eq!(cart.memory.prg_peek(0x8000), 16, "bank switched");

        let config = bincode::config::legacy();
        let bytes = bincode::serde::encode_to_vec(&cart.memory, config).expect("memory serializes");
        let (mut restored, _) =
            bincode::serde::decode_from_slice::<crate::memory::Memory, _>(&bytes, config)
                .expect("memory deserializes");

        assert_eq!(
            restored.prg_peek(0x8000),
            0,
            "page tables must not survive serialization"
        );
        mapper.sync(&mut restored);
        assert_eq!(
            restored.prg_peek(0x8000),
            16,
            "sync must rebuild the mapping from mapper registers"
        );
    }

    /// Nametable mapping lives in the CHR page table, which is also skipped by serde, so a board
    /// that does not restore mirroring in `sync` comes back with unmapped nametables and renders
    /// from a zero-filled page.
    #[test]
    fn sync_restores_nametable_mirroring() {
        for four_screen in [false, true] {
            let mut cart = Cart::empty_sized(0x8000, 0x2000);
            if four_screen {
                cart.header.flags |= 0x08;
            }
            cart.memory.set_mirroring(cart.mirroring());
            let mut mapper = Nrom::load(&mut cart).expect("valid mapper");

            cart.memory.chr_write(0x2000, 0x5A);
            assert_eq!(cart.memory.chr_peek(0x2000), 0x5A);

            let config = bincode::config::legacy();
            let bytes =
                bincode::serde::encode_to_vec(&cart.memory, config).expect("memory serializes");
            let (mut restored, _) =
                bincode::serde::decode_from_slice::<crate::memory::Memory, _>(&bytes, config)
                    .expect("memory deserializes");

            mapper.sync(&mut restored);
            assert_eq!(
                restored.chr_peek(0x2000),
                0x5A,
                "sync must restore nametable mirroring (four_screen: {four_screen})"
            );
        }
    }
}
