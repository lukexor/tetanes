//! Memory Mappers for cartridges.
//!
//! <https://wiki.nesdev.org/w/index.php/Mapper>

use crate::{
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sample},
    fs,
    memory::{Memory, Src},
    ppu::Mirroring,
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::path::Path;

bitflags! {
    /// Which of a board's optional hooks apply, resolved once at cart load and cached beside the
    /// mapper (see `Ppu::mapper_ops`) so the hot paths that would otherwise dispatch
    /// unconditionally into every board - `Bus::cpu_clock`, `Cpu::handle_interrupts`,
    /// `Ppu::chr_read`/`chr_peek`, `Ppu::notify_ppu_bus`, `Bus::read`/`peek` - can gate each one
    /// on a bit test instead.
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[must_use]
    pub struct MapperOps: u8 {
        /// Board needs `Clock::clock()` called every CPU cycle (an IRQ or serial-write timing
        /// counter, expansion audio, etc).
        const CLOCKED = 1 << 0;
        /// Board can raise `Map::irq_pending()`.
        const IRQ = 1 << 1;
        /// Board produces audio via `Sample::output()`.
        const AUDIO = 1 << 2;
        /// Board can raise `Map::dma_pending()`.
        const DMA = 1 << 3;
        /// Board must observe every PPU bus address.
        ///
        /// A board does not see PPU reads themselves, so the A12 rising-edge scanline counters
        /// (MMC3, FK23C) and CHR latches (MMC2, MMC4) need the PPU to notify them explicitly via
        /// [`Map::ppu_bus_addr`]. Whatever this reaches runs thousands of times a frame: re-map
        /// only what changed, never [`Map::sync`].
        const WATCHES_PPU_BUS = 1 << 4;
        /// Board serves some CPU reads itself rather than from page tables.
        ///
        /// Expansion hardware - Namco163's audio registers and IRQ counter, Bandai's EEPROM and
        /// barcode reader - is not memory and cannot be expressed as a page. Served through
        /// [`Map::prg_read`]/[`Map::prg_peek`].
        const SERVES_PRG_READS = 1 << 5;
        /// Board serves some PPU reads itself rather than from page tables.
        ///
        /// MMC5 is the only board that needs this: in extended-attribute mode the CHR bank for a
        /// tile comes from a byte of ExRAM looked up per tile, and attribute and fill-mode reads
        /// are synthesised rather than fetched, so neither is expressible as a page entry. Served
        /// through [`Map::chr_read`]/[`Map::chr_peek`].
        const SERVES_CHR_READS = 1 << 6;
    }
}

// Shared board logic, not boards in their own right, so they are not in the `boards!` table.
pub mod mmc1;
pub mod mmc3;
pub mod vrc_irq;

pub use mmc1::{Mmc1, Revision as Mmc1Revision};
pub use mmc3::{Mmc3, Revision as Mmc3Revision};
// `boards!` re-exports each board type itself; a board module exporting anything *else* publicly -
// so far only a revision enum - lists it here.
pub use m024_m026_vrc6::Revision as Vrc6Revision;
pub use m071_bf909x::Revision as Bf909Revision;

/// Errors that mappers can return while loading.
///
/// Empty for now: banking used to validate window sizes against a `Banks` table and could fail,
/// but a board on page tables only writes page entries, and every out-of-range bank wraps within
/// its region by construction. The type and the `Result` stay so that a board needing to reject a
/// cart - a bad NES 2.0 submapper, say - can do so without a breaking change.
#[derive(thiserror::Error, Debug)]
#[must_use]
pub enum Error {}

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

/// Everything the rest of the crate needs to know about a board, in one row each.
///
/// A row is `Variant(StorageType) in module { <mapper numbers> => <loader> }`, and generating from
/// it means **adding a board is two edits: its own file, and one row here.**
///
/// Previously it was six: the file, four separate lists in this module (`pub mod`, `pub use`, the
/// enum variant, the `From` impl, the dispatch arm), the `match` in `Cart::new`, the audio arm in
/// `Sample for Mapper`, and the layout entry in `lib.rs`'s `print_layouts`. Each failed differently
/// when forgotten, and two of them failed only at runtime - a board left out of `Cart::new` loaded
/// as `Mapper::none()` and read as open bus, and one left out of `Sample for Mapper` was silent.
///
/// Notes on the row syntax:
/// - The storage type is spelled out rather than inferred, because it is the one thing that
///   genuinely varies: large boards are `Box`ed to keep `Mapper` small (see `print_layouts`).
///   `From<Board>` is generated either way, so `board.into()` works regardless.
/// - **Row order is the enum's variant order, which `bincode` serializes by index.** Reordering
///   rows silently invalidates every existing save state. Add new boards at the end.
/// - Loader arms are emitted in row order into one `match`, with `cart` bound to `&mut Cart`. Where
///   two boards share a mapper number they carry mutually exclusive guards rather than relying on
///   arm order, so that the constraint above and this dispatch cannot conflict.
macro_rules! boards {
    ($cart:ident: $(
        $(#[$meta:meta])*
        $variant:ident($($storage:tt)+) in $module:ident {
            $($num:pat $(if $guard:expr)? => $load:expr),+ $(,)?
        }
    ),+ $(,)?) => {
        $(pub mod $module;)+
        $(pub use $module::$variant;)+

        /// A `Mapper` is a specific cart variant with dedicated memory mapping logic for memory
        /// addressing and bank switching.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[must_use]
        pub enum Mapper {
            None(()),
            $($(#[$meta])* $variant($($storage)+),)+
        }

        impl Mapper {
            /// Pick and load the board a cart's mapper number calls for.
            ///
            /// An unrecognised number is not an error: it yields [`Mapper::none`], which reads as
            /// open bus, so an unsupported ROM still loads and reports itself rather than failing.
            pub fn from_cart($cart: &mut Cart) -> Result<Self, Error> {
                Ok(match $cart.mapper_num() {
                    $($($num $(if $guard)? => $load?,)+)+
                    _ => Self::none(),
                })
            }
        }

        $(impl_from_board!($variant, $($storage)+);)+

        impl_dispatch!($($variant),+);

        /// Board sizes for `lib.rs`'s `print_layouts`, which watches `Mapper` for cache behaviour.
        ///
        /// Deliberately the unboxed size: what matters is how large the variant would be inline,
        /// which is what decides whether it should be `Box`ed.
        #[cfg(test)]
        pub(crate) const BOARD_LAYOUTS: &[(&str, usize)] =
            &[$((stringify!($variant), size_of::<$variant>()),)+];
    };
}

/// Implement `From<Board>` for `Mapper`, boxing on the way in when the variant is boxed.
macro_rules! impl_from_board {
    ($variant:ident, Box<$board:ident>) => {
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
    ($variant:ident, $board:ident) => {
        impl From<$board> for Mapper {
            fn from(board: $board) -> Self {
                Self::$variant(board)
            }
        }
    };
}

/// Forward every [`Map`] method from `Mapper` to the selected board.
///
/// The methods are inherent rather than an `impl Map for Mapper`, because `Map` carries the
/// `clock`/`reset`/`region`/`output` methods a board used to get from `Clock`/`Reset`/`Regional`/
/// `Sample` supertraits, and `Mapper` still implements those traits to forward them down the
/// ownership tree - implementing both would make every one of those call sites ambiguous.
macro_rules! impl_dispatch {
    ($($variant:ident),+ $(,)?) => {
        impl Mapper {
            /// An empty Mapper.
            pub const fn none() -> Self {
                Self::None(())
            }

            /// Whether mapper is `None`.
            pub const fn is_none(&self) -> bool {
                matches!(self, Self::None(_))
            }

            /// Which of the optional per-cycle hooks this board needs.
            pub fn mapper_ops(&self) -> MapperOps {
                dispatch!(self, [$($variant),+], m => m.mapper_ops())
            }

            /// Synchronize a write to a PPU address.
            pub fn ppu_write(&mut self, addr: u16, val: u8) {
                dispatch!(self, [$($variant),+], m => m.ppu_write(addr, val))
            }

            /// Whether an IRQ is pending acknowledgement.
            pub fn irq_pending(&self) -> bool {
                dispatch!(self, [$($variant),+], m => m.irq_pending())
            }

            /// Whether an DMA is pending acknowledgement.
            pub fn dma_pending(&self) -> bool {
                dispatch!(self, [$($variant),+], m => m.dma_pending())
            }

            /// Clear pending DMA.
            pub fn clear_dma_pending(&mut self) {
                dispatch!(self, [$($variant),+], m => m.clear_dma_pending())
            }

            /// Returns the current [`Mirroring`] mode.
            #[inline(always)]
            pub fn mirroring(&self) -> Mirroring {
                dispatch!(self, [$($variant),+], m => m.mirroring())
            }

            /// Handle a CPU-space write, re-banking as needed.
            #[inline(always)]
            pub fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
                dispatch!(self, [$($variant),+], m => m.write_register(memory, addr, val))
            }

            /// Write this board's battery-backed state.
            pub fn save_sram(&self, memory: &Memory, path: &Path) -> fs::Result<()> {
                dispatch!(self, [$($variant),+], m => m.save_sram(memory, path))
            }

            /// Restore state previously written by `save_sram`.
            pub fn load_sram(&mut self, memory: &mut Memory, path: &Path) -> fs::Result<()> {
                dispatch!(self, [$($variant),+], m => m.load_sram(memory, path))
            }

            /// Serve a CPU read, returning `None` to fall through to page-table memory.
            #[inline(always)]
            pub fn prg_read(&mut self, addr: u16) -> Option<u8> {
                dispatch!(self, [$($variant),+], m => m.prg_read(addr))
            }

            /// Side-effect-free form of [`Map::prg_read`].
            #[inline(always)]
            pub fn prg_peek(&self, addr: u16) -> Option<u8> {
                dispatch!(self, [$($variant),+], m => m.prg_peek(addr))
            }

            /// Serve a PPU read, returning `None` to fall through to page-table memory.
            #[inline(always)]
            pub fn chr_read(&mut self, memory: &mut Memory, addr: u16) -> Option<u8> {
                dispatch!(self, [$($variant),+], m => m.chr_read(memory, addr))
            }

            /// Side-effect-free form of [`Map::chr_read`].
            #[inline(always)]
            pub fn chr_peek(&self, memory: &Memory, addr: u16) -> Option<u8> {
                dispatch!(self, [$($variant),+], m => m.chr_peek(memory, addr))
            }

            /// Observe a PPU bus address.
            #[inline(always)]
            pub fn ppu_bus_addr(&mut self, memory: &mut Memory, addr: u16) {
                dispatch!(self, [$($variant),+], m => m.ppu_bus_addr(memory, addr))
            }

            /// Rebuild the page tables from this board's register state.
            pub fn sync(&mut self, memory: &mut Memory) {
                dispatch!(self, [$($variant),+], m => m.sync(memory))
            }
        }

        impl Sample for Mapper {
            /// Output a single audio sample.
            #[inline]
            fn output(&self) -> f32 {
                dispatch!(self, [$($variant),+], m => m.output())
            }
        }

        impl Reset for Mapper {
            /// Reset the component given the [`ResetKind`].
            fn reset(&mut self, kind: ResetKind) {
                dispatch!(self, [$($variant),+], m => m.reset(kind))
            }
        }

        impl Clock for Mapper {
            /// Clock component once.
            #[inline]
            fn clock(&mut self) {
                dispatch!(self, [$($variant),+], m => m.clock())
            }
        }

        impl Regional for Mapper {
            /// Return the current region.
            fn region(&self) -> NesRegion {
                dispatch!(self, [$($variant),+], m => m.region())
            }

            /// Set the region.
            fn set_region(&mut self, region: NesRegion) {
                dispatch!(self, [$($variant),+], m => m.set_region(region))
            }
        }
    };
}

/// One `match` over every `Mapper` variant, running the same call against each.
///
/// The call is taken whole (`m => m.sync(memory)`) rather than as a method name plus an argument
/// list, because an argument repetition nested inside the per-variant repetition is a
/// "meta-variable repeats N times, but M times" error - the two have different depths.
macro_rules! dispatch {
    ($self:expr, [$($variant:ident),+], $board:ident => $call:expr) => {
        match $self {
            Mapper::None($board) => $call,
            $(Mapper::$variant($board) => $call,)+
        }
    };
}

boards! {
    // Binds the `&mut Cart` that every loader and guard below refers to as `cart`. Named here
    // rather than inside the macro because macro hygiene would otherwise put the two identifiers
    // in different syntax contexts.
    cart:

    /// `NROM` (Mapper 000)
    Nrom(Nrom) in m000_nrom { 0 => Nrom::load(cart) },
    /// `SxROM`/`MMC1` (Mappers 001, 155)
    Sxrom(Sxrom) in m001_sxrom {
        1 => Sxrom::load(cart, Mmc1Revision::BC),
        155 => Sxrom::load(cart, Mmc1Revision::A),
    },
    /// `UxROM` (Mapper 002)
    Uxrom(Uxrom) in m002_uxrom { 2 => Uxrom::load(cart) },
    /// `CNROM` (Mapper 003)
    Cnrom(Cnrom) in m003_cnrom { 3 => Cnrom::load(cart) },
    /// `TxROM`/`MMC3` (Mappers 004, 076, 088, 095, 154, 206)
    Txrom(Txrom) in m004_txrom {
        4 | 76 | 88 | 95 | 154 | 206 => Txrom::load(cart),
    },
    /// `ExROM`/`MMC5` (Mapper 005)
    Exrom(Box<Exrom>) in m005_exrom { 5 => Exrom::load(cart) },
    /// `AxROM` (Mapper 007)
    Axrom(Axrom) in m007_axrom { 7 => Axrom::load(cart) },
    /// `PxROM`/`MMC2` (Mapper 009)
    Pxrom(Pxrom) in m009_pxrom { 9 => Pxrom::load(cart) },
    /// `FxROM`/`MMC4` (Mapper 010)
    Fxrom(Fxrom) in m010_fxrom { 10 => Fxrom::load(cart) },
    /// `Color Dreams` (Mappers 011, 144)
    ColorDreams(ColorDreams) in m011_color_dreams {
        11 | 144 => ColorDreams::load(cart),
    },
    /// `Bandai FCG` (Mappers 016, 153, 157, and 159)
    BandaiFCG(Box<BandaiFCG>) in bandai_fcg {
        16 | 153 | 157 | 159 => BandaiFCG::load(cart),
    },
    /// `Jaleco SS88006` (Mapper 018)
    JalecoSs88006(JalecoSs88006) in m018_jalecoss88006 {
        18 => JalecoSs88006::load(cart),
    },
    /// `Namco163` (Mappers 019, 210)
    Namco163(Box<Namco163>) in m019_namco163 { 19 | 210 => Namco163::load(cart) },
    /// `VRC6` (Mappers 024, 026)
    Vrc6(Box<Vrc6>) in m024_m026_vrc6 {
        24 => Vrc6::load(cart, Vrc6Revision::A),
        26 => Vrc6::load(cart, Vrc6Revision::B),
    },
    /// `BNROM` (Mapper 034)
    // Mapper 034 is two different boards; <= 8K of CHR-ROM implies BNROM.
    Bnrom(Bnrom) in m034_bnrom {
        34 if cart.chr_rom_size < 0x4000 => Bnrom::load(cart),
    },
    /// `NINA-001` (Mapper 034)
    Nina001(Nina001) in m034_nina001 {
        34 if cart.chr_rom_size >= 0x4000 => Nina001::load(cart),
    },
    /// `GxROM` (Mapper 066)
    Gxrom(Gxrom) in m066_gxrom { 66 => Gxrom::load(cart) },
    /// `Sunsoft FME7` (Mapper 069)
    SunsoftFme7(SunsoftFme7) in m069_sunsoft_fme7 { 69 => SunsoftFme7::load(cart) },
    /// `Bf909x` (Mapper 071)
    Bf909x(Bf909x) in m071_bf909x { 71 => Bf909x::load(cart) },
    /// `NINA-003`/`NINA-006` (Mappers 079, 113, 146)
    Nina003006(Nina003006) in m079_nina003_006 {
        79 | 113 | 146 => Nina003006::load(cart),
    },
    /// `NES-EVENT` (Mapper 105)
    NesEvent(NesEvent) in m105_nes_event {
        105 => NesEvent::load(cart, [false, false, true, false]),
    },
    /// `Waixing FK23C`/`FS303` (Mapper 176)
    // Boxed from when it was 280 bytes; the port to page tables left it holding only registers, so
    // `print_layouts` now reports 56 and it no longer drives the enum's size (SunsoftFme7's 72
    // does). Left boxed for now - unboxing is a cache-behaviour question to measure, not assume.
    Fk23C(Box<Fk23C>) in m176_fk23c { 176 => Fk23C::load(cart) },
}

impl Default for Mapper {
    fn default() -> Self {
        Self::none()
    }
}

/// Trait implemented by every board a [`Mapper`] can hold.
///
/// Boards are pure register state: every read is served from [`Memory`]'s page tables, which a
/// board rewrites from [`Map::write_register`] and [`Map::sync`]. The only reads that reach a
/// board are the ones no page entry can describe - expansion hardware and MMC5's synthesised
/// fetches - and those go through [`Map::prg_read`] and [`Map::chr_read`], which return `None` to
/// mean "ordinary memory, read the page table". Each is gated on a cached `MapperOps` bit so the
/// boards without any pay a bit test rather than a dispatch.
///
/// Every method has a default, so a board writes exactly the ones its hardware has and nothing
/// else. That includes [`Map::clock`], [`Map::reset`] and [`Map::region`]/[`Map::set_region`],
/// which used to come from `Clock`/`Reset`/`Regional` supertrait bounds and cost every board an
/// empty `impl` for each one it did not need - four boilerplate impls per board, of which only
/// [`Exrom`] ever filled in `Regional` and none at all filled in `Sram`. [`Mapper`] still
/// implements those traits to forward them down the ownership tree; the boards no longer do.
pub trait Map {
    /// Which of the optional per-cycle hooks this board needs: a per-cycle `Clock::clock()`, an
    /// IRQ, expansion audio, or DMA. Resolved once at cart load into `Ppu::mapper_ops`, so a board
    /// that needs none of them costs a bit test rather than a dispatch on every CPU cycle.
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::empty()
    }

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
    ///
    /// Reported for debuggers and for boards to read back; the nametables themselves are page
    /// entries, applied through [`Memory::set_mirroring`] from a board's `sync`.
    // All mappers have mirroring, even if it's hard-wired.
    fn mirroring(&self) -> Mirroring;

    /// Handle a CPU-space write, re-banking as needed.
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

    /// Serve a CPU read, returning `None` to fall through to page-table memory.
    ///
    /// Only reached when the board's `mapper_ops()` includes `MapperOps::SERVES_PRG_READS`.
    fn prg_read(&mut self, _addr: u16) -> Option<u8> {
        None
    }

    /// Side-effect-free form of [`Map::prg_read`], for debuggers.
    fn prg_peek(&self, _addr: u16) -> Option<u8> {
        None
    }

    /// Serve a PPU read, returning `None` to fall through to page-table memory.
    ///
    /// Only reached when the board's `mapper_ops()` includes `MapperOps::SERVES_CHR_READS`. Runs
    /// *before* the page-table read, so a board may also re-bank here: MMC5 swaps its sprite and
    /// background CHR bank sets partway through a scanline, and the swap has to apply to the fetch
    /// that triggered it.
    fn chr_read(&mut self, _memory: &mut Memory, _addr: u16) -> Option<u8> {
        None
    }

    /// Side-effect-free form of [`Map::chr_read`], for debuggers.
    fn chr_peek(&self, _memory: &Memory, _addr: u16) -> Option<u8> {
        None
    }

    /// Observe a PPU bus address, for boards whose `mapper_ops()` includes
    /// `MapperOps::WATCHES_PPU_BUS`.
    fn ppu_bus_addr(&mut self, _memory: &mut Memory, _addr: u16) {}

    /// Rebuild the page tables from this board's register state.
    ///
    /// Page tables are derived state and are not serialized, so they must be reconstructed after
    /// loading a save state; a board's registers survive, the mapping does not. Also called after
    /// reset, where `Reset` has no access to [`Memory`]. Boards implement their initial mapping
    /// here and call it from `load`, so a fresh cart and a restored save state take the same path.
    fn sync(&mut self, _memory: &mut Memory) {}

    /// Clock the board once, for boards whose `mapper_ops()` includes `MapperOps::CLOCKED`.
    fn clock(&mut self) {}

    /// Reset the board given the [`ResetKind`].
    ///
    /// [`Memory`] is not reachable here, so a board that re-banks on reset sets its registers and
    /// leaves the re-mapping to the [`Map::sync`] that follows.
    fn reset(&mut self, _kind: ResetKind) {}

    /// Return the board's region.
    ///
    /// Only boards whose timing differs between regions - MMC5's audio - track one.
    fn region(&self) -> NesRegion {
        NesRegion::default()
    }

    /// Set the board's region.
    fn set_region(&mut self, _region: NesRegion) {}

    /// Output one expansion-audio sample, for boards whose `mapper_ops()` includes
    /// `MapperOps::AUDIO`.
    ///
    /// Only MMC5, Namco163, VRC6 and FME7 have any; every other board is silent and never reaches
    /// this, because `Bus::cpu_clock` checks the flag first.
    fn output(&self) -> f32 {
        0.0
    }
}

impl Map for () {
    fn mirroring(&self) -> Mirroring {
        Mirroring::default()
    }
}

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
