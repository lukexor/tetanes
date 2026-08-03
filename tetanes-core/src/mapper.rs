//! Memory mappers for cartridges - the board hardware that decides what a CPU or PPU address
//! actually reaches.
//!
//! <https://wiki.nesdev.org/w/index.php/Mapper>
//!
//! [`Mapper`] is an enum with static dispatch rather than a boxed trait object; this is deliberate,
//! since board dispatch sits on the hottest paths in the emulator. Each board implements [`Map`],
//! and the [`Mapper`] holding it hangs off the [`Bus`] alongside [`Memory`] - the
//! PPU is the heaviest user, but the CPU reaches PRG through them too.
//!
//! # Adding a board
//!
//! Four edits, two of which the compiler will not remind you about.
//!
//! First, `src/mapper/m0NN_<name>.rs`, named for the board's primary (lowest) mapper number -
//! logic shared by a family lives in an un-numbered file instead (`mmc1.rs`, `mmc3.rs`,
//! `vrc_irq.rs`). A board is a plain serializable struct with a `load` constructor and a [`Map`]
//! impl. UxROM, in `m002_uxrom.rs`, is about as small as they get:
//!
//! ```ignore
//! pub struct Uxrom { pub mirroring: Mirroring, pub prg_bank: u8 }
//!
//! impl Uxrom {
//!     pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
//!         let mut board = Self { mirroring: cart.mirroring(), prg_bank: 0 };
//!         board.update_banks(&mut cart.memory); // establish the power-on banking
//!         Ok(board.into())                      // `From<Uxrom> for Mapper` is generated
//!     }
//! }
//!
//! impl Map for Uxrom {
//!     fn mirroring(&self) -> Mirroring { self.mirroring }
//!
//!     fn write_register(&mut self, memory: &mut Memory, _addr: u16, val: u8) {
//!         self.prg_bank = val & 0x0F;
//!         self.update_banks(memory);
//!     }
//!
//!     fn update_banks(&mut self, memory: &mut Memory) {
//!         memory.map_prg(0x8000, Self::PRG_WINDOW, i32::from(self.prg_bank), Src::PrgRom);
//!         memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom); // -1 = last bank
//!         memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
//!         memory.set_mirroring(self.mirroring);
//!     }
//! }
//! ```
//!
//! Only [`Map::mirroring`] is required - everything else defaults, so a board writes exactly the
//! hooks its hardware has. The two that matter most are [`Map::write_register`], called for every
//! CPU write in `$4020..=$FFFF`, and [`Map::update_banks`], which republishes the whole banking
//! layout into the page tables from the board's current registers.
//!
//! Keeping all banking in `update_banks` is what lets save states restore correctly: page tables
//! are derived state and are not serialized, so
//! [`Bus::rebuild_mapper_state`](crate::bus::Bus::rebuild_mapper_state) rebuilds them by replaying
//! `update_banks` against the restored registers. A board that banks anywhere *else* will load a
//! broken state.
//!
//! Beyond that: [`Map::clock`] for boards with a per-cycle IRQ counter, [`Map::irq_pending`],
//! [`Map::output`] for expansion audio, [`Map::prg_read`]/[`Map::chr_read`] as escape hatches for
//! things no page entry can express, and [`Map::save_sram`]/[`Map::load_sram`] when the battery
//! covers something other than PRG-RAM. Declare whichever of the per-cycle hooks you use in
//! [`Map::mapper_ops`], or they will not be called.
//!
//! Second, one row in the `boards!` table below, in mapper-number order:
//!
//! ```ignore
//! Uxrom(Uxrom) = 2 in m002_uxrom { 2 => Uxrom::load(cart) },
//! ```
//!
//! That row generates the `pub mod`, the `pub use`, the [`Mapper`] variant, the [`From`] impls,
//! every dispatch arm, the mapper-number match in [`Mapper::from_cart`], and the `print_layouts`
//! entry. Note:
//!
//! - **`= 2` is the stable serialization id: assign-once, never reused, never renumbered.** It is
//!   what goes on disk. Rows may therefore be reordered freely. A board sharing a mapper number
//!   with an earlier one takes `0x1000 + n` instead.
//! - Spell the storage as `Board(Box<Board>)` for a large board, to keep [`Mapper`] small. Boxing
//!   is a *measured* trade - it costs an indirection on boards clocked every CPU cycle, and both
//!   directions have surprised us. See `benches/README.md`.
//! - A board exporting something besides the board type (so far only a revision enum) needs a
//!   `pub use` next to the table.
//!
//! A mapper number no row claims is [`Error::Unimplemented`], so an unsupported ROM says so rather
//! than loading as open bus and showing a black screen.
//!
//! Third, an arm in `ControlDeck::update_mapper_revisions`, whose match over [`Mapper`] is
//! exhaustive: a board with a user-selectable revision calls its `set_revision`, everything else
//! joins the no-op list. The compiler catches this one.
//!
//! Fourth, a row in the supported-mapper table in the workspace `README.md`. Nothing catches that
//! one, which is why it is the step that gets skipped.
//!
//! Optionally add a `test_roms!` group in `common.rs`.

use crate::{
    bus::Bus,
    cart::Cart,
    common::{NesRegion, ResetKind},
    fs,
    memory::{Memory, Src},
    ppu::Mirroring,
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::path::Path;

bitflags! {
    /// Which of a board's optional hooks apply, resolved once at cart load and cached beside the
    /// mapper (see `Bus::mapper_ops`) so the hot paths that would otherwise dispatch
    /// unconditionally into every board - `Bus::cpu_clock`, `Bus::handle_interrupts`,
    /// `Bus::chr_read`/`chr_peek`, `Bus::ppu_bus_read`, `Bus::read`/`peek` - can gate each one
    /// on a bit test instead.
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[must_use]
    pub struct MapperOps: u8 {
        /// Board needs [`Map::clock`] called every CPU cycle (an IRQ or serial-write timing
        /// counter, expansion audio, etc).
        const CLOCKED = 1 << 0;
        /// Board can raise `Map::irq_pending()`.
        const IRQ = 1 << 1;
        /// Board produces audio via [`Map::output`].
        const AUDIO = 1 << 2;
        /// Board can raise `Map::dma_pending()`.
        const DMA = 1 << 3;
        /// Board must observe every PPU bus address.
        ///
        /// A board does not see PPU reads themselves, so the A12 rising-edge scanline counters
        /// (MMC3, FK23C) and CHR latches (MMC2, MMC4) need the PPU to notify them explicitly via
        /// [`Map::ppu_bus_addr`]. Whatever this reaches runs thousands of times a frame: re-map
        /// only what changed, never [`Map::update_banks`].
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
pub use m021_vrc24::Revision as Vrc24Revision;
pub use m024_m026_vrc6::Revision as Vrc6Revision;
pub use m033_m048_taito_tc0190::Revision as TaitoRevision;
pub use m071_bf909x::Revision as Bf909Revision;

/// Errors that mappers can return while loading.
///
/// Loading a *supported* board is infallible: a board only writes page entries, and every
/// out-of-range bank wraps within its region by construction. The `Result` stays so that a board
/// needing to reject a cart - a bad NES 2.0 submapper, say - can do so without a breaking change.
#[derive(thiserror::Error, Debug)]
#[must_use]
pub enum Error {
    /// No board in the `boards!` table serves this mapper number.
    ///
    /// Saying so beats loading as [`Mapper::none`] and reading as open bus, which starts the ROM
    /// and shows a black screen with nothing said about why.
    #[error("unimplemented mapper: {0}")]
    Unimplemented(u16),
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

/// Everything the rest of the crate needs to know about a board, in one row each.
///
/// A row is `Variant(StorageType) in module { <mapper numbers> => <loader> }`.
///
/// It generates the `pub mod`, the `pub use`, the enum variant, the [`From`] impl, every dispatch
/// arm, the `match` in [`Mapper::from_cart`], the audio arm in [`Mapper::output`] and the layout
/// entry in `lib.rs`'s `print_layouts` - six lists that, kept by hand, each fail differently when
/// one is forgotten, two of them only at runtime: a board missing from [`Mapper::from_cart`] loads
/// as [`Mapper::none()`](Mapper::none) and reads as open bus, and one missing from
/// [`Mapper::output`] is silent.
///
/// Notes on the row syntax:
/// - The storage type is spelled out rather than inferred, because it is the one thing that
///   genuinely varies: large boards are `Box`ed to keep `Mapper` small (see `print_layouts`).
///   `From<Board>` is generated either way, so `board.into()` works regardless.
/// - `= <id>` is the board's **stable serialization id**: assign-once, never reused, never
///   renumbered. It is what goes on disk, so **rows may be freely reordered** - keep them in
///   mapper-number order. See [`Mapper`]'s `Serialize`/`Deserialize` for why this is hand-rolled.
///
///   The id is the board's *primary* (lowest) mapper number, so the table reads as its own index.
///   That cannot be the whole rule, because the mapping is not one-to-one in either direction:
///   `Txrom` alone serves 4, 76, 88, 95, 154 and 206, and BNROM and NINA-001 *both* serve 34. A
///   board sharing a number with an earlier one takes `0x1000 + n` instead, above every real NES
///   2.0 mapper number. `Mapper::none()` is `0xFFFF`, since 0 is NROM.
/// - Loader arms are emitted in row order into one `match`, with `cart` bound to `&mut Cart`. Where
///   two boards share a mapper number they carry mutually exclusive guards rather than relying on
///   arm order, so that reordering rows cannot change which board a ROM gets.
macro_rules! boards {
    ($cart:ident: $(
        $(#[$meta:meta])*
        $variant:ident($($storage:tt)+) = $id:literal in $module:ident {
            $($num:pat $(if $guard:expr)? => $load:expr),+ $(,)?
        }
    ),+ $(,)?) => {
        $(pub mod $module;)+
        $(pub use $module::$variant;)+

        /// A `Mapper` is a specific cart variant with dedicated memory mapping logic for memory
        /// addressing and bank switching.
        //
        // `Serialize`/`Deserialize` are hand-rolled below rather than derived, so that the
        // serialized tag is the `boards!` row's explicit id instead of the variant's declaration
        // position. Without that, reordering rows silently reinterprets every existing save state.
        #[derive(Debug, Clone)]
        #[must_use]
        pub enum Mapper {
            /// No board, i.e. no cart loaded. Reads come back as open bus.
            None(()),
            $($(#[$meta])* $variant($($storage)+),)+
        }

        impl Mapper {
            /// Pick and load the board a cart's mapper number calls for.
            ///
            /// # Errors
            ///
            /// Returns [`Error::Unimplemented`] if no board serves this mapper number.
            pub fn from_cart($cart: &mut Cart) -> Result<Self, Error> {
                Ok(match $cart.mapper_num() {
                    $($($num $(if $guard)? => $load?,)+)+
                    num => return Err(Error::Unimplemented(num)),
                })
            }
        }

        /// Every board's name, for `Deserialize` in self-describing formats.
        const VARIANTS: &[&str] = &["None", $(stringify!($variant),)+];

        /// [`Mapper::none`]'s id. Not 0, because that is NROM.
        const NONE_ID: u32 = 0xFFFF;
        const NONE_ID_U64: u64 = NONE_ID as u64;

        /// Serialize as the `boards!` row's stable id, not the variant's declaration position.
        ///
        /// serde's derive always uses the position, and honours neither an explicit discriminant
        /// nor `#[repr]` - `enum E { A = 10 }` still serializes as `0`. bincode 2's own non-serde
        /// derive behaves the same way. So the stability has to live here, in our code, where it
        /// survives changing serializer. Self-describing formats still see the variant name.
        impl Serialize for Mapper {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                match self {
                    Self::None(m) => serializer.serialize_newtype_variant("Mapper", NONE_ID, "None", m),
                    $(Self::$variant(m) => serializer.serialize_newtype_variant(
                        "Mapper", $id, stringify!($variant), m,
                    ),)+
                }
            }
        }

        impl<'de> Deserialize<'de> for Mapper {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                use serde::de::{EnumAccess, VariantAccess, Visitor};
                use std::fmt;

                /// Which board a tag names, resolved from a stable id or from a name.
                enum Tag {
                    None,
                    $($variant,)+
                }

                impl<'de> Deserialize<'de> for Tag {
                    fn deserialize<D: serde::Deserializer<'de>>(
                        deserializer: D,
                    ) -> Result<Self, D::Error> {
                        struct TagVisitor;

                        impl Visitor<'_> for TagVisitor {
                            type Value = Tag;

                            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                                f.write_str("a mapper board id or name")
                            }

                            fn visit_u64<E: serde::de::Error>(self, id: u64) -> Result<Tag, E> {
                                match id {
                                    NONE_ID_U64 => Ok(Tag::None),
                                    $($id => Ok(Tag::$variant),)+
                                    // A save state written by a newer build that knows a board
                                    // this one does not.
                                    _ => Err(E::custom(format_args!("unknown mapper id: {id}"))),
                                }
                            }

                            fn visit_str<E: serde::de::Error>(self, name: &str) -> Result<Tag, E> {
                                match name {
                                    "None" => Ok(Tag::None),
                                    $(stringify!($variant) => Ok(Tag::$variant),)+
                                    _ => Err(E::unknown_variant(name, VARIANTS)),
                                }
                            }
                        }

                        deserializer.deserialize_identifier(TagVisitor)
                    }
                }

                struct MapperVisitor;

                impl<'de> Visitor<'de> for MapperVisitor {
                    type Value = Mapper;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a Mapper")
                    }

                    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Mapper, A::Error> {
                        let (tag, board) = data.variant::<Tag>()?;
                        Ok(match tag {
                            Tag::None => Mapper::None(board.newtype_variant()?),
                            $(Tag::$variant => Mapper::$variant(board.newtype_variant()?),)+
                        })
                    }
                }

                deserializer.deserialize_enum("Mapper", VARIANTS, MapperVisitor)
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

        /// Each board's stable serialization id, for the uniqueness test.
        #[cfg(test)]
        pub(crate) const BOARD_IDS: &[(&str, u32)] = &[$((stringify!($variant), $id),)+];
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
// Inherent rather than an `impl Map for Mapper`: `Map` carries `clock`/`reset`/`region`/`output`
// and `Mapper` has inherent methods of the same names to forward down the ownership tree, so
// implementing the trait too would make every one of those call sites ambiguous.
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

            /// Check what a board says it does against what it does, in debug builds.
            ///
            /// [`Map::mapper_ops`] is written by hand per board and nothing links it to the
            /// methods that board overrode, so implementing a hook and forgetting its bit
            /// compiles and runs with the hardware silently dead. The read hooks can be settled
            /// outright: sweep every address [`Bus`](crate::bus::Bus) would route to the board and
            /// require one that declared nothing to answer nothing. `CLOCKED` is the gap - a board
            /// that is never clocked and a board whose clock does nothing look the same from here.
            ///
            /// Only the boards that declared *no* hook are swept, so this cannot be slow in a way
            /// that depends on the game: the hooks it calls are the trait's own defaults.
            #[cfg(debug_assertions)]
            pub(crate) fn check_mapper_ops(&self, memory: &Memory) {
                let name = match self {
                    Mapper::None(_) => "None",
                    $(Mapper::$variant(_) => stringify!($variant),)+
                };
                let ops = self.mapper_ops();
                if !ops.intersects(MapperOps::SERVES_PRG_READS) {
                    for addr in 0x4100..=0xFFFFu16 {
                        assert!(
                            self.prg_peek(addr).is_none(),
                            "{name} answers CPU ${addr:04X}, but its mapper_ops() omits \
                             SERVES_PRG_READS, so the bus never asks it",
                        );
                    }
                }
                if !ops.intersects(MapperOps::SERVES_CHR_READS) {
                    for addr in 0x0000..=0x3EFFu16 {
                        assert!(
                            self.chr_peek(memory, addr).is_none(),
                            "{name} answers PPU ${addr:04X}, but its mapper_ops() omits \
                             SERVES_CHR_READS, so the bus never asks it",
                        );
                    }
                }
                assert!(
                    ops.intersects(MapperOps::AUDIO) || self.output() == 0.0,
                    "{name} outputs audio, but its mapper_ops() omits AUDIO",
                );
                assert!(
                    ops.intersects(MapperOps::IRQ) || !self.irq_pending(),
                    "{name} raises an IRQ, but its mapper_ops() omits IRQ",
                );
                assert!(
                    ops.intersects(MapperOps::DMA) || !self.dma_pending(),
                    "{name} raises a DMA, but its mapper_ops() omits DMA",
                );
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

            /// Append this board's register state to `out`, for a debugger to display.
            pub fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
                dispatch!(self, [$($variant),+], m => m.registers(out))
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
            pub fn update_banks(&mut self, memory: &mut Memory) {
                dispatch!(self, [$($variant),+], m => m.update_banks(memory))
            }
        }

        impl Mapper {
            /// Output a single audio sample.
            #[inline]
            pub fn output(&self) -> f32 {
                dispatch!(self, [$($variant),+], m => m.output())
            }
        }

        impl Mapper {
            /// Reset the component given the [`ResetKind`].
            pub fn reset(&mut self, kind: ResetKind) {
                dispatch!(self, [$($variant),+], m => m.reset(kind))
            }
        }

        impl Mapper {
            /// Clock component once.
            #[inline]
            pub fn clock(&mut self) {
                dispatch!(self, [$($variant),+], m => m.clock())
            }
        }

        impl Mapper {
            /// Return the current region.
            pub fn region(&self) -> NesRegion {
                dispatch!(self, [$($variant),+], m => m.region())
            }

            /// Set the region.
            pub fn set_region(&mut self, region: NesRegion) {
                dispatch!(self, [$($variant),+], m => m.set_region(region))
            }
        }
    };
}

/// One `match` over every `Mapper` variant, running the same call against each.
///
/// The call is taken whole (`m => m.update_banks(memory)`) rather than as a method name plus an argument
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
    Nrom(Nrom) = 0 in m000_nrom { 0 => Nrom::load(cart) },
    /// `SxROM`/`MMC1` (Mappers 001, 155)
    Sxrom(Sxrom) = 1 in m001_sxrom {
        1 => Sxrom::load(cart, Mmc1Revision::BC),
        155 => Sxrom::load(cart, Mmc1Revision::A),
    },
    /// `UxROM` (Mapper 002)
    Uxrom(Uxrom) = 2 in m002_uxrom { 2 => Uxrom::load(cart) },
    /// `CNROM` (Mapper 003)
    Cnrom(Cnrom) = 3 in m003_cnrom { 3 => Cnrom::load(cart) },
    /// `TxROM`/`MMC3` (Mappers 004, 076, 088, 095, 154, 206)
    Txrom(Txrom) = 4 in m004_txrom {
        4 | 76 | 88 | 95 | 154 | 206 => Txrom::load(cart),
    },
    /// `ExROM`/`MMC5` (Mapper 005)
    Exrom(Box<Exrom>) = 5 in m005_exrom { 5 => Exrom::load(cart) },
    /// `AxROM` (Mapper 007)
    Axrom(Axrom) = 7 in m007_axrom { 7 => Axrom::load(cart) },
    /// `PxROM`/`MMC2` (Mapper 009)
    Pxrom(Pxrom) = 9 in m009_pxrom { 9 => Pxrom::load(cart) },
    /// `FxROM`/`MMC4` (Mapper 010)
    Fxrom(Fxrom) = 10 in m010_fxrom { 10 => Fxrom::load(cart) },
    /// `Color Dreams` (Mappers 011, 144)
    ColorDreams(ColorDreams) = 11 in m011_color_dreams {
        11 | 144 => ColorDreams::load(cart),
    },
    /// `Bandai FCG` (Mappers 016, 153, 157, and 159)
    BandaiFCG(Box<BandaiFCG>) = 16 in bandai_fcg {
        16 | 153 | 157 | 159 => BandaiFCG::load(cart),
    },
    /// `Jaleco SS88006` (Mapper 018)
    JalecoSs88006(JalecoSs88006) = 18 in m018_jalecoss88006 {
        18 => JalecoSs88006::load(cart),
    },
    /// `Namco163` (Mappers 019, 210)
    Namco163(Box<Namco163>) = 19 in m019_namco163 { 19 | 210 => Namco163::load(cart) },
    /// `VRC2`/`VRC4` (Mappers 021, 022, 023, 025)
    Vrc24(Vrc24) = 21 in m021_vrc24 { 21 | 22 | 23 | 25 => Vrc24::load(cart) },
    /// `VRC6` (Mappers 024, 026)
    Vrc6(Box<Vrc6>) = 24 in m024_m026_vrc6 {
        24 => Vrc6::load(cart, Vrc6Revision::A),
        26 => Vrc6::load(cart, Vrc6Revision::B),
    },
    /// `Taito TC0190`/`TC0350` (Mapper 033) and `TC0690` (Mapper 048)
    TaitoTc0190(TaitoTc0190) = 33 in m033_m048_taito_tc0190 {
        33 => TaitoTc0190::load(cart, TaitoRevision::Tc0190),
        48 => TaitoTc0190::load(cart, TaitoRevision::Tc0690),
    },
    /// `BNROM` (Mapper 034)
    // Mapper 034 is two different boards; <= 8K of CHR-ROM implies BNROM.
    Bnrom(Bnrom) = 34 in m034_bnrom {
        34 if cart.chr_rom_size < 0x4000 => Bnrom::load(cart),
    },
    /// `NINA-001` (Mapper 034)
    Nina001(Nina001) = 4096 in m034_nina001 {
        34 if cart.chr_rom_size >= 0x4000 => Nina001::load(cart),
    },
    /// `GxROM` (Mapper 066)
    Gxrom(Gxrom) = 66 in m066_gxrom { 66 => Gxrom::load(cart) },
    /// `Sunsoft FME7` (Mapper 069)
    SunsoftFme7(Box<SunsoftFme7>) = 69 in m069_sunsoft_fme7 { 69 => SunsoftFme7::load(cart) },
    /// `Bf909x` (Mapper 071)
    Bf909x(Bf909x) = 71 in m071_bf909x { 71 => Bf909x::load(cart) },
    /// `NINA-003`/`NINA-006` (Mappers 079, 113, 146)
    Nina003006(Nina003006) = 79 in m079_nina003_006 {
        79 | 113 | 146 => Nina003006::load(cart),
    },
    /// `NES-EVENT` (Mapper 105)
    NesEvent(NesEvent) = 105 in m105_nes_event {
        105 => NesEvent::load(cart, [false, false, true, false]),
    },
    /// `Waixing FK23C`/`FS303` (Mapper 176)
    // Holds only registers, so `print_layouts` reports 56 and it is not what drives the enum's size
    // (SunsoftFme7's 72 is). Kept boxed anyway - unboxing is a cache-behaviour question to measure,
    // not assume.
    Fk23C(Fk23C) = 176 in m176_fk23c { 176 => Fk23C::load(cart) },
}

impl Default for Mapper {
    fn default() -> Self {
        Self::none()
    }
}

/// Installing a board and keeping the state derived from it in step. Takes the [`Bus`] rather than
/// the [`Mapper`] because both the board and the [`Memory`] it banks hang off the bus.
impl Bus {
    /// Install a board, replacing whatever cart was loaded.
    #[inline]
    pub fn load_mapper(&mut self, mapper: Mapper) {
        self.mapper_ops = mapper.mapper_ops();
        self.mapper = mapper;
        // `ControlDeck::load_rom` sets the region *before* installing the cart, so a mapper whose
        // timing depends on it (MMC5's expansion audio) would otherwise never be told.
        self.mapper.set_region(self.region);
        #[cfg(debug_assertions)]
        self.mapper.check_mapper_ops(&self.memory);
    }

    /// Notify the mapper of a PPU bus address, for A12 scanline counters and CHR latches.
    ///
    /// Reads made through `Bus::chr_read` notify the board themselves; this exists for the sites
    /// that move the PPU address without fetching through it, such as `$2006` writes.
    #[inline(always)]
    pub fn notify_ppu_bus(&mut self, addr: u16) {
        if self.mapper_ops.intersects(MapperOps::WATCHES_PPU_BUS) {
            let Self { mapper, memory, .. } = self;
            mapper.ppu_bus_addr(memory, addr);
        }
    }

    /// Rebuild the page tables from the mapper's register state.
    ///
    /// Required after loading a save state: page tables are derived state and are not serialized,
    /// so without this a restored state would have every page unmapped.
    pub fn rebuild_mapper_state(&mut self) {
        let Self { mapper, memory, .. } = self;
        mapper.update_banks(memory);
        // mapper_ops is #[serde(skip)] - a restored save state replaced the whole `Bus`, so this
        // is the state-load path's chance to recompute it from the (serialized, and thus correct)
        // mapper.
        self.mapper_ops = self.mapper.mapper_ops();
        #[cfg(debug_assertions)]
        self.mapper.check_mapper_ops(&self.memory);
    }
}

/// Trait implemented by every board a [`Mapper`] can hold.
///
/// Boards are pure register state: every read is served from [`Memory`]'s page tables, which a
/// board rewrites from [`Map::write_register`] and [`Map::update_banks`]. The only reads that reach a
/// board are the ones no page entry can describe - expansion hardware and MMC5's synthesised
/// fetches - and those go through [`Map::prg_read`] and [`Map::chr_read`], which return `None` to
/// mean "ordinary memory, read the page table". Each is gated on a cached `MapperOps` bit so the
/// boards without any pay a bit test rather than a dispatch.
///
/// Every method has a default, so a board writes exactly the ones its hardware has and nothing
/// else - including [`Map::clock`], [`Map::reset`] and [`Map::region`]/[`Map::set_region`], which
/// most boards do not need at all.
pub trait Map {
    /// Which of the optional per-cycle hooks this board needs: a per-cycle [`Map::clock`], an IRQ,
    /// expansion audio, or DMA. Resolved once at cart load into `Bus::mapper_ops`, so a board that
    /// needs none of them costs a bit test rather than a dispatch on every CPU cycle.
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
    /// entries, applied through [`Memory::set_mirroring`] from a board's `update_banks`.
    // All mappers have mirroring, even if it's hard-wired.
    fn mirroring(&self) -> Mirroring;

    /// Append this board's register state to `out`, for a debugger to display.
    ///
    /// Names and values, so a mapper pane needs no per-board code: whatever a board reports is
    /// what gets shown. Writing into a caller-owned buffer rather than returning one keeps a view
    /// that refreshes every frame from allocating, and lets a board report values it computes
    /// rather than only ones it stores.
    ///
    /// What is worth reporting is what a person debugging a game would ask for - the bank
    /// registers, the IRQ state, whatever latch the board keeps - not every field. The default
    /// reports nothing, which is right for a board that has no registers.
    fn registers(&self, _out: &mut Vec<(&'static str, u32)>) {}

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
    ///
    /// An override writes through [`fs::save_sram`], which stamps [`fs::SRAM_VERSION`] rather than
    /// the save-state version: a player's battery saves outlive the save-state format, so the two
    /// must be able to move independently.
    fn save_sram(&self, memory: &Memory, path: &Path) -> fs::Result<()> {
        fs::save_sram(path, &memory.region_ref(Src::PrgRam).to_vec())
    }

    /// Restore state previously written by [`Map::save_sram`].
    fn load_sram(&mut self, memory: &mut Memory, path: &Path) -> fs::Result<()> {
        let data = fs::load_sram::<Vec<u8>>(path)?;
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
    fn update_banks(&mut self, _memory: &mut Memory) {}

    /// Clock the board once, for boards whose `mapper_ops()` includes `MapperOps::CLOCKED`.
    fn clock(&mut self) {}

    /// Reset the board given the [`ResetKind`].
    ///
    /// [`Memory`] is not reachable here, so a board that re-banks on reset sets its registers and
    /// leaves the re-mapping to the [`Map::update_banks`] that follows.
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
pub(crate) mod test_utils {
    //! Shared scaffolding for the per-board banking tests.
    //!
    //! A board's job is to put the right physical bank behind the right CPU/PPU address, so these
    //! build a cart whose every 1K page is filled with its own index. A single `peek` then names
    //! the bank it was served from, and a test reads as "writing $8000 = 3 puts PRG page 3 at
    //! $8000" instead of as an opaque hash.

    use super::*;
    use crate::{
        cart::Cart,
        memory::{MemoryLayout, PAGE_SIZE},
    };

    /// A `Cart` with page-indexed contents: PRG-ROM page `n` is filled with `n`, CHR page `n` with
    /// `0x80 | n`, so PRG and CHR reads can never be mistaken for each other.
    ///
    /// Sizes are in bytes. A `chr` of 0 gives 8K of CHR-RAM instead of CHR-ROM. Keep regions under
    /// 128K: page indices are written as bytes and the CHR tag costs the top bit.
    pub fn page_indexed_cart(prg_rom: usize, prg_ram: usize, chr: usize) -> Cart {
        let chr_writable = chr == 0;
        let chr_size = if chr_writable { 8 * 1024 } else { chr };
        let mut cart = Cart::empty_sized(prg_rom, chr);
        cart.memory = Memory::new(MemoryLayout {
            prg_rom,
            prg_ram,
            chr: chr_size,
            chr_writable,
            ciram: 2 * 1024,
            ex_ram: 0,
        });
        for (page, data) in cart
            .memory
            .region_mut(Src::PrgRom)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            data.fill(page as u8);
        }
        for (page, data) in cart
            .memory
            .region_mut(Src::Chr)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            data.fill(0x80 | page as u8);
        }
        // Distinguishable from both, so "did PRG-RAM land at $6000" needs no separate setup.
        cart.memory.region_mut(Src::PrgRam).fill(0x5A);
        cart
    }

    /// The CPU addresses [`Bus`](crate::bus::Bus) routes to the board. Below this the address
    /// decodes to console hardware - the APU and the controller ports - and the board is never
    /// consulted, so a test that uses one is asserting about an access no game can make.
    const CPU_BUS_TO_BOARD: std::ops::RangeInclusive<u16> = 0x4100..=0xFFFF;
    /// The PPU addresses that reach the board. `$3F00-$3FFF` is palette RAM inside the PPU.
    const PPU_BUS_TO_BOARD: std::ops::RangeInclusive<u16> = 0x0000..=0x3EFF;

    /// Mirrors `Bus::write`: the data store happens first, then the board acts on the register.
    pub fn write(mapper: &mut Mapper, cart: &mut Cart, addr: u16, val: u8) {
        assert!(
            CPU_BUS_TO_BOARD.contains(&addr),
            "${addr:04X} does not reach the board on the CPU bus"
        );
        cart.memory.prg_write(addr, val);
        mapper.write_register(&mut cart.memory, addr, val);
    }

    /// Mirrors `Bus::peek`'s routing for a page-table board: the board's escape hatch first - and
    /// only if it declared one - then the page tables.
    ///
    /// The `mapper_ops()` gate is the point. `Bus` will not call a hook a board did not declare,
    /// so a helper that calls it unconditionally lets a board pass its own tests while doing
    /// nothing in the emulator.
    pub fn prg_peek(mapper: &Mapper, cart: &Cart, addr: u16) -> u8 {
        assert!(
            CPU_BUS_TO_BOARD.contains(&addr),
            "${addr:04X} does not reach the board on the CPU bus"
        );
        mapper
            .mapper_ops()
            .intersects(MapperOps::SERVES_PRG_READS)
            .then(|| mapper.prg_peek(addr))
            .flatten()
            .unwrap_or_else(|| cart.memory.prg_peek(addr))
    }

    /// Mirrors `Bus::chr_peek`'s routing for a page-table board, `mapper_ops()` gate included.
    pub fn chr_peek(mapper: &Mapper, cart: &Cart, addr: u16) -> u8 {
        assert!(
            PPU_BUS_TO_BOARD.contains(&addr),
            "${addr:04X} does not reach the board on the PPU bus"
        );
        mapper
            .mapper_ops()
            .intersects(MapperOps::SERVES_CHR_READS)
            .then(|| mapper.chr_peek(&cart.memory, addr))
            .flatten()
            .unwrap_or_else(|| cart.memory.chr_peek(addr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{cart::Cart, memory::Src};

    /// MMC5's expansion audio has two region-dependent clocks, so `Bus::set_region` has to reach
    /// the board and not stop at the components it owns directly - a board that never sees the
    /// region runs at whatever its cart was constructed with.
    #[test]
    fn setting_the_region_reaches_the_mapper() {
        use crate::{bus::Bus, cart::Cart};

        let mut cart = Cart::empty_sized(0x8000, 0x2000);
        cart.header.mapper_num = 5;
        cart.mapper = Mapper::from_cart(&mut cart).expect("MMC5 loads");

        let mut bus = Bus::default();
        bus.set_region(NesRegion::Ntsc);
        bus.load_cart(cart);
        assert_eq!(bus.mapper.region(), NesRegion::Ntsc, "region at load");

        bus.set_region(NesRegion::Pal);
        assert_eq!(
            bus.mapper.region(),
            NesRegion::Pal,
            "a later region change must reach the mapper too"
        );
    }

    /// The serialized tag must be the `boards!` row's stable id, not the variant's declaration
    /// position - that is the whole reason `Serialize`/`Deserialize` are hand-rolled rather than
    /// derived. serde's derive always uses the position and honours neither an explicit
    /// discriminant nor `#[repr]`.
    ///
    /// Moving a row must leave this passing; renumbering an id must make it fail.
    #[test]
    fn variant_tag_is_the_stable_id_not_the_declaration_position() {
        let config = bincode::config::legacy();
        // The id is the board's primary mapper number: NROM 0, UxROM 2.
        let cases = [
            (Mapper::none(), 0xFFFF),
            (
                Mapper::from(Nrom {
                    mirroring: Mirroring::Vertical,
                }),
                0,
            ),
            (
                Mapper::from(Uxrom {
                    mirroring: Mirroring::Vertical,
                    prg_bank: 0,
                }),
                2,
            ),
        ];
        for (mapper, expected_id) in cases {
            let bytes = bincode::serde::encode_to_vec(&mapper, config).expect("serializes");
            let tag = u32::from_le_bytes(bytes[..4].try_into().expect("4-byte tag"));
            assert_eq!(
                tag, expected_id,
                "{mapper:?} must serialize with id {expected_id}"
            );

            let (restored, _) = bincode::serde::decode_from_slice::<Mapper, _>(&bytes, config)
                .expect("round trips");
            assert_eq!(
                std::mem::discriminant(&restored),
                std::mem::discriminant(&mapper),
                "{mapper:?} must come back as the same board"
            );
        }
    }

    /// An id this build does not know must be a clean error, not a panic or the wrong board.
    #[test]
    fn an_unknown_board_id_fails_to_deserialize() {
        let config = bincode::config::legacy();
        let mapper = Mapper::from(Nrom {
            mirroring: Mirroring::Vertical,
        });
        let mut bytes = bincode::serde::encode_to_vec(&mapper, config).expect("serializes");
        bytes[..4].copy_from_slice(&9999u32.to_le_bytes());
        assert!(
            bincode::serde::decode_from_slice::<Mapper, _>(&bytes, config).is_err(),
            "an unknown board id must not decode"
        );
    }

    /// Two boards sharing an id means one silently loads as the other.
    #[test]
    fn board_ids_are_unique_and_not_reserved() {
        let mut ids = BOARD_IDS.to_vec();
        ids.sort_unstable_by_key(|&(_, id)| id);
        for pair in ids.windows(2) {
            assert_ne!(
                pair[0].1, pair[1].1,
                "{} and {} share stable id {}",
                pair[0].0, pair[1].0, pair[0].1
            );
        }
        for &(board, id) in BOARD_IDS {
            assert_ne!(
                id, NONE_ID,
                "{board} collides with Mapper::none()'s reserved id"
            );
        }
    }

    /// A debugger's mapper pane shows whatever a board reports, so the names have to be usable and
    /// the buffer has to be appended to rather than replaced - a caller collecting several boards,
    /// or reusing one buffer across frames, gets nonsense otherwise.
    #[test]
    fn reported_registers_are_named_and_appended() {
        let mut out = vec![("sentinel", 0)];
        for &(board, id) in BOARD_IDS {
            // The stable id is the board's primary mapper number, except for the handful above
            // every real one that exist only to disambiguate a shared number.
            let Ok(num) = u16::try_from(id) else { continue };
            let mut cart = test_utils::page_indexed_cart(128 * 1024, 8 * 1024, 128 * 1024);
            cart.header.mapper_num = num;
            // A board that will not load from a generic cart - one guarding on a CHR size, say -
            // is not what this test is about.
            let Ok(mapper) = Mapper::from_cart(&mut cart) else {
                continue;
            };
            let before = out.len();
            mapper.registers(&mut out);
            assert_eq!(out[0], ("sentinel", 0), "{board} replaced the buffer");
            for &(name, _) in &out[before..] {
                assert!(!name.is_empty(), "{board} reported an unnamed register");
            }
            let names = &out[before..];
            for (i, &(name, _)) in names.iter().enumerate() {
                assert!(
                    !names[..i].iter().any(|&(other, _)| other == name),
                    "{board} reported {name:?} twice, so a pane cannot tell them apart"
                );
            }
        }
    }

    /// An unsupported mapper number must report itself rather than loading as open bus.
    #[test]
    fn an_unimplemented_mapper_number_is_an_error() {
        let mut cart = Cart::empty_sized(0x8000, 0x2000);
        cart.header.mapper_num = 999;
        let err = Mapper::from_cart(&mut cart).expect_err("999 is not implemented");
        assert!(
            matches!(err, Error::Unimplemented(999)),
            "expected Unimplemented(999), got {err:?}"
        );
    }

    /// Page tables are `#[serde(skip)]` derived state, and `Cpu::load` replaces the whole console
    /// when a save state is loaded. Without an `update_banks` on the way back in, every page comes back
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
        // ROM is not serialized either; `Cpu::load` reattaches it from the running console.
        assert!(restored.restore_rom_from(&cart.memory), "same cart");
        mapper.update_banks(&mut restored);
        assert_eq!(
            restored.prg_peek(0x8000),
            16,
            "sync must rebuild the mapping from mapper registers"
        );
    }

    /// Nametable mapping lives in the CHR page table, which is also skipped by serde, so a board
    /// that does not restore mirroring in `update_banks` comes back with unmapped nametables and renders
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

            mapper.update_banks(&mut restored);
            assert_eq!(
                restored.chr_peek(0x2000),
                0x5A,
                "sync must restore nametable mirroring (four_screen: {four_screen})"
            );
        }
    }
}
