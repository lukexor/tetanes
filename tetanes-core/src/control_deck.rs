//! [`ControlDeck`], the primary entry point for emulating an NES.
//!
//! A deck owns the whole console - CPU, PPU, APU, the cartridge and its mapper - and is driven one
//! frame at a time:
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use tetanes_core::prelude::*;
//!
//! let mut deck = ControlDeck::new();
//! deck.load_rom_path("some_awesome_game.nes")?;
//!
//! while deck.is_running() {
//!     // A *display* frame is however many NES frames the emulation speed asks for, so clock
//!     // until it says the display frame is done. At 1x that is one pass.
//!     while deck.clock_frame()? == Clocked::Continue {}
//!     let samples = deck.audio_samples(); // queue to an audio device
//!     let frame = deck.frame_buffer();    // blit to the screen
//! }
//! # Ok(())
//! # }
//! ```
//!
//! [`ControlDeck::clock_frame`] is the one you want; [`ControlDeck::clock_scanline`],
//! [`ControlDeck::clock_instr`] and [`ControlDeck::clock_seconds`] step at finer granularities for
//! debuggers and tests. Each of them refreshes what [`ControlDeck::frame_buffer`] and
//! [`ControlDeck::audio_samples`] report, and discards what the previous call produced - so audio
//! cannot silently accumulate. See [`Config::clear_audio_on_clock`] to opt out of that.
//!
//! Behavior that would otherwise be a per-call argument lives on the deck instead:
//! [`ControlDeck::set_run_ahead`] for latency hiding, [`ControlDeck::set_frame_speed`] for
//! emulation speed, [`ControlDeck::set_filter`] for video filtering. Everything a [`Config`] sets
//! up front also has a setter, so it can be changed on a running deck.
//!
//! # Stability
//!
//! This is the crate's primary public API: [`ControlDeck`]'s fields are private, and is what
//! semantic versioning covers. Reaching past it ([`ControlDeck::bus`] and [`ControlDeck::bus_mut`])
//! is supported and is how a debugger or a cheat engine is meant to work, but what it reaches
//! tracks implementation instead of the crate version. See the crate-level
//! [stability](crate#stability) note for the three tiers and what decides which one a type is in.

use crate::sys::fs as sys_fs;
use crate::{
    apu::{self, Apu, Channel},
    bus::{self, Bus},
    cart::{self, Cart},
    common::{NesRegion, ResetKind},
    debug::{Debugger, PcHistory},
    fs,
    genie::{self, GenieCode},
    input::{FourPlayer, Joypad, Player},
    mapper::{self, Bf909Revision, Mapper, MapperRevision, Mmc3Revision},
    memory::RamState,
    patch::Patch,
    ppu,
    video::{Frame, Video, VideoFilter},
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;
use tracing::{error, info};

/// Result returned from [`ControlDeck`] methods.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that [`ControlDeck`] can return.
#[derive(Error, Debug)]
#[must_use]
#[non_exhaustive]
pub enum Error {
    /// [`Cart`] error when loading a ROM.
    #[error(transparent)]
    Cart(#[from] cart::Error),
    /// Battery-backed RAM error.
    #[error("sram error: {0:?}")]
    Sram(fs::Error),
    /// Save state error.
    #[error("save state error: {0:?}")]
    SaveState(fs::Error),
    /// When trying to load a save state that doesn't exist.
    #[error("no save state found")]
    NoSaveStateFound,
    /// Operational error indicating a ROM must be loaded first.
    #[error("no rom is loaded")]
    RomNotLoaded,
    /// CPU state is corrupted and emulation can't continue. Could be due to a bad ROM image or a
    /// corrupt save state.
    #[error("cpu state is corrupted")]
    CpuCorrupted,
    /// Invalid Game Genie code error.
    #[error(transparent)]
    InvalidGenieCode(#[from] genie::Error),
    /// Invalid file path.
    #[error("invalid file path {0:?}")]
    InvalidFilePath(PathBuf),
    /// The ROM's mapper number has no board implementation.
    #[error("unimplemented mapper `{0}`")]
    UnimplementedMapper(u16),
    /// A save state that does not belong to the loaded cart.
    #[error(transparent)]
    StateMismatch(#[from] crate::cpu::StateMismatch),
    /// Filesystem error.
    #[error(transparent)]
    Fs(#[from] fs::Error),
    /// IO error.
    #[error("{context}: {source:?}")]
    Io {
        /// What was being done when this happened.
        context: String,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

impl Error {
    /// Wraps an I/O error with a description of what was being done.
    pub fn io(source: std::io::Error, context: impl Into<String>) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

bitflags! {
    /// Headless mode flags to disable audio and video processing, reducing CPU usage.
    #[derive(Default, Debug, Copy, Clone, PartialEq, Serialize, Deserialize, )]
    #[must_use]
    pub struct HeadlessMode: u8 {
        /// Disable audio mixing.
        const NO_AUDIO = 0x01;
        /// Disable pixel rendering.
        const NO_VIDEO = 0x02;
    }
}

/// Set of desired mapper revisions to use when loading a ROM matching the available mapper types.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
#[non_exhaustive]
pub struct MapperRevisionsConfig {
    /// MMC3 mapper revision.
    pub mmc3: Mmc3Revision,
    /// BF909 mapper revision.
    pub bf909: Bf909Revision,
}

impl MapperRevisionsConfig {
    /// Set the desired mapper revision to use when loading a ROM matching the available mapper types.
    pub const fn set(&mut self, rev: MapperRevision) {
        match rev {
            MapperRevision::Mmc3(rev) => self.mmc3 = rev,
            MapperRevision::Bf909(rev) => self.bf909 = rev,
        }
    }
}

/// Control deck configuration settings.
///
/// ```
/// use tetanes_core::prelude::*;
///
/// let cfg = Config::default()
///     .with_sram_dir(None)
///     .with_ram_state(RamState::AllZeros);
/// assert!(cfg.sram_dir.is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
#[non_exhaustive]
pub struct Config {
    /// Video filter.
    pub filter: VideoFilter,
    /// NES region.
    pub region: NesRegion,
    /// RAM initialization state.
    pub ram_state: RamState,
    /// Four player adapter.
    pub four_player: FourPlayer,
    /// Enable zapper gun.
    pub zapper: bool,
    /// Game Genie codes.
    pub genie_codes: Vec<GenieCode>,
    /// Whether to support concurrent D-Pad input which wasn't possible on the original NES.
    pub concurrent_dpad: bool,
    /// Apu channels enabled.
    pub channels_enabled: [bool; Apu::MAX_CHANNEL_COUNT],
    /// Headless mode.
    pub headless_mode: HeadlessMode,
    /// Where battery-backed cart RAM is kept, and whether it is kept at all.
    ///
    /// `Some` - the default - means loading a cart restores its battery file and unloading writes
    /// it back, the way the hardware behaves. `None` is for an embedder whose own frontend owns
    /// save files: the deck then performs no filesystem access of its own, and
    /// [`ControlDeck::save_sram`] / [`ControlDeck::load_sram`] are the way in and out.
    pub sram_dir: Option<PathBuf>,
    /// Which mapper revisions to emulate for any ROM loaded that uses this mapper.
    pub mapper_revisions: MapperRevisionsConfig,
    /// Whether to emulate PPU warmup where writes to certain registers are ignored. Can result in
    /// some games not working correctly.
    ///
    /// See: <https://www.nesdev.org/wiki/PPU_power_up_state>
    pub emulate_ppu_warmup: bool,
    /// How many frames [`ControlDeck::clock_frame`] runs ahead of the console to hide input lag.
    ///
    /// `0`, the default, disables it. See [`ControlDeck::set_run_ahead`].
    pub run_ahead: usize,
    /// Whether clocking discards the previous call's audio samples first.
    ///
    /// `true`, the default, means [`ControlDeck::audio_samples`] holds exactly what the most recent
    /// clock produced. Set it to `false` to accumulate samples across several clock calls, in which
    /// case you must call [`ControlDeck::clear_audio_samples`] yourself or the buffer grows without
    /// bound.
    pub clear_audio_on_clock: bool,
}

impl Config {
    /// Base directory for storing TetaNES data.
    pub const BASE_DIR: &'static str = "tetanes";
    /// Directory for storing battery-backed Cart RAM.
    pub const SRAM_DIR: &'static str = "sram";
    /// File extension for battery-backed Cart RAM.
    pub const SRAM_EXTENSION: &'static str = "sram";

    /// Returns the default directory where TetaNES data is stored.
    #[inline]
    #[must_use]
    pub fn default_data_dir() -> PathBuf {
        dirs::data_local_dir().map_or_else(|| PathBuf::from("data"), |dir| dir.join(Self::BASE_DIR))
    }

    /// Returns the default directory used to store battery-backed Cart RAM.
    #[inline]
    #[must_use]
    pub fn default_sram_dir() -> PathBuf {
        Self::default_data_dir().join(Self::SRAM_DIR)
    }

    /// Sets the video filter.
    pub const fn with_filter(mut self, filter: VideoFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the NES region.
    pub const fn with_region(mut self, region: NesRegion) -> Self {
        self.region = region;
        self
    }

    /// Sets the RAM initialization state.
    pub const fn with_ram_state(mut self, ram_state: RamState) -> Self {
        self.ram_state = ram_state;
        self
    }

    /// Sets the four player adapter.
    pub const fn with_four_player(mut self, four_player: FourPlayer) -> Self {
        self.four_player = four_player;
        self
    }

    /// Sets whether the zapper gun is connected.
    pub const fn with_zapper(mut self, zapper: bool) -> Self {
        self.zapper = zapper;
        self
    }

    /// Sets the Game Genie codes to apply on load.
    pub fn with_genie_codes(mut self, genie_codes: Vec<GenieCode>) -> Self {
        self.genie_codes = genie_codes;
        self
    }

    /// Sets whether concurrent D-Pad input is allowed.
    pub const fn with_concurrent_dpad(mut self, concurrent_dpad: bool) -> Self {
        self.concurrent_dpad = concurrent_dpad;
        self
    }

    /// Sets which APU channels are enabled.
    pub const fn with_channels_enabled(
        mut self,
        channels_enabled: [bool; Apu::MAX_CHANNEL_COUNT],
    ) -> Self {
        self.channels_enabled = channels_enabled;
        self
    }

    /// Sets headless mode.
    pub const fn with_headless_mode(mut self, headless_mode: HeadlessMode) -> Self {
        self.headless_mode = headless_mode;
        self
    }

    /// Sets where battery-backed cart RAM is kept.
    pub fn with_sram_dir(mut self, sram_dir: Option<PathBuf>) -> Self {
        self.sram_dir = sram_dir;
        self
    }

    /// Sets the mapper revisions to emulate.
    pub const fn with_mapper_revisions(mut self, mapper_revisions: MapperRevisionsConfig) -> Self {
        self.mapper_revisions = mapper_revisions;
        self
    }

    /// Sets whether PPU warmup is emulated.
    pub const fn with_emulate_ppu_warmup(mut self, emulate_ppu_warmup: bool) -> Self {
        self.emulate_ppu_warmup = emulate_ppu_warmup;
        self
    }

    /// Sets how many frames to run ahead of the console.
    pub const fn with_run_ahead(mut self, run_ahead: usize) -> Self {
        self.run_ahead = run_ahead;
        self
    }

    /// Sets whether clocking discards the previous call's audio samples first.
    pub const fn with_clear_audio_on_clock(mut self, clear_audio_on_clock: bool) -> Self {
        self.clear_audio_on_clock = clear_audio_on_clock;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            filter: VideoFilter::default(),
            region: NesRegion::Auto,
            ram_state: RamState::Random,
            four_player: FourPlayer::default(),
            zapper: false,
            genie_codes: Vec::new(),
            concurrent_dpad: false,
            channels_enabled: [true; Apu::MAX_CHANNEL_COUNT],
            headless_mode: HeadlessMode::empty(),
            sram_dir: Some(Self::default_sram_dir()),
            mapper_revisions: MapperRevisionsConfig::default(),
            emulate_ppu_warmup: false,
            run_ahead: 0,
            clear_audio_on_clock: true,
        }
    }
}

/// What one call to [`ControlDeck::clock_frame`] did.
///
/// A *display* frame is however many NES frames the emulation speed asks for, so the two are not
/// the same thing and this says which one just finished.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[must_use]
pub enum Clocked {
    /// Clocked a frame; this display frame owes at least one more. Call again.
    Continue,
    /// Clocked a frame, and it was the last one this display frame owed.
    Complete,
    /// Clocked nothing: this display frame owed none. Only reachable below 1x speed.
    Idle,
}

/// Represents a loaded ROM [`Cart`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadedRom {
    /// Name of ROM.
    pub name: String,
    /// Whether the loaded Cart is battery-backed.
    pub battery_backed: bool,
    /// Auto-detected of the loaded Cart.
    pub region: NesRegion,
}

/// What [`ControlDeck`] recycles between run-ahead frames.
///
/// Run-ahead rewinds the console *past* the frame it wants to display, which would take that
/// frame's pixels with it - so `pending` parks them where [`ControlDeck::frame_buffer`] can still
/// reach them afterwards. `spare` holds the console's real frame meanwhile, and comes back on
/// restore.
///
/// All three exist to be reused rather than allocated:
/// [`Buffer`](crate::ppu::frame::Buffer)'s `Default` allocates and zeroes 120 KiB, and a console's
/// arena is the size of a cartridge, both of which this path would otherwise pay on every frame.
#[derive(Debug, Clone, Default)]
struct RunAheadFrames {
    /// The pixels of the frame that should be displayed, parked here before the console is rewound.
    pending: crate::ppu::frame::Buffer,
    /// Frame number `pending` was rendered as. The NTSC filter is phase-dependent on it, so it has
    /// to travel with the pixels rather than being read back off the rewound console.
    pending_frame_number: u32,
    /// Whether `pending` holds the frame to display. Cleared at the start of every clock.
    pending_valid: bool,
    /// The console's own frame, parked while the run-ahead frames render.
    spare: crate::ppu::frame::Buffer,
    /// The console the last run-ahead left behind, kept to snapshot into again. `None` until
    /// run-ahead first runs. See `Bus::snapshot_from`.
    snapshot: Option<Box<Bus>>,
}

/// Represents an NES Control Deck. Encapsulates the entire emulation state.
#[derive(Debug, Clone)]
#[must_use]
pub struct ControlDeck {
    /// Whether a ROM is loaded and the emulation is currently running or not.
    running: bool,
    /// Video output and filtering.
    video: Video,
    /// Whether `video.frame` still needs the filter applied, letting `frame_buffer` be cached when
    /// called more than once between clocks.
    video_frame_stale: bool,
    /// How many frames to run ahead of the console to hide input lag. `0` disables it.
    run_ahead: usize,
    /// Frame buffers recycled by run-ahead. `None` until run-ahead is first used, so a deck that
    /// never enables it never allocates them.
    run_ahead_frames: Option<Box<RunAheadFrames>>,
    /// Whether clocking discards the previous call's audio samples first.
    clear_audio_on_clock: bool,
    /// The currently loaded ROM [`Cart`], if any.
    loaded_rom: Option<LoadedRom>,
    /// Where battery-backed Cart RAM is kept, or `None` to perform no filesystem access.
    sram_dir: Option<PathBuf>,
    /// Mapper revisions to emulate for any ROM loaded that matches the given mappers.
    mapper_revisions: MapperRevisionsConfig,
    /// Whether to auto-detect the region based on the loaded Cart.
    auto_detect_region: bool,
    /// Remaining CPU cycles to execute used to clock a given number of seconds.
    cycles_remaining: f32,
    /// Emulated frame speed step ranging from 1 (0.25 speed) to 8 (2.0).
    frame_speed_step: u8,
    /// Accumulated frame speed to account for slower 1x speeds.
    frame_accumulator: u8,
    /// NES frames the display frame in progress still owes; 0 means the next
    /// [`ControlDeck::clock_frame`] begins a new one.
    frames_to_clock: u8,
    /// The console: CPU, PPU, APU, input and the cart's board.
    bus: Bus,
}

impl Default for ControlDeck {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlDeck {
    /// Byte layout every console state is written in, whichever entry point writes it.
    //
    // Here rather than at each call site so an embedder cannot pick a different one and get a
    // state this crate will not read back. Fixint, so a state's size depends on the cart's layout
    // and not on the values in it - which is what lets `serialized_state_len` be asked once.
    const STATE_CONFIG: bincode::config::Configuration<
        bincode::config::LittleEndian,
        bincode::config::Fixint,
    > = bincode::config::legacy();

    /// Creates a NES `ControlDeck` with the default configuration.
    ///
    /// It has no cartridge yet, so [`ControlDeck::is_running`] is `false` until a ROM is loaded
    /// with [`ControlDeck::load_rom`] or [`ControlDeck::load_rom_path`].
    ///
    /// ```
    /// use tetanes_core::prelude::*;
    ///
    /// let deck = ControlDeck::new();
    /// assert!(!deck.is_running());
    /// assert!(deck.loaded_rom().is_none());
    /// ```
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    /// Creates a NES `ControlDeck` with the given [`Config`].
    ///
    /// ```
    /// use tetanes_core::prelude::*;
    /// use tetanes_core::control_deck::{Config, HeadlessMode};
    ///
    /// // A deck for batch processing: no video or audio work, deterministic RAM.
    /// let deck = ControlDeck::with_config(
    ///     Config::default()
    ///         .with_headless_mode(HeadlessMode::NO_AUDIO | HeadlessMode::NO_VIDEO)
    ///         .with_ram_state(RamState::AllZeros),
    /// );
    /// assert_eq!(deck.region(), NesRegion::Ntsc);
    /// ```
    pub fn with_config(cfg: Config) -> Self {
        let mut bus = Bus::new(cfg.region, cfg.ram_state);
        bus.ppu.skip_rendering = cfg.headless_mode.contains(HeadlessMode::NO_VIDEO);
        bus.ppu.emulate_warmup = cfg.emulate_ppu_warmup;
        bus.apu.skip_mixing = cfg.headless_mode.contains(HeadlessMode::NO_AUDIO);
        if cfg.region.is_auto() {
            bus.set_region(NesRegion::default());
        } else {
            bus.set_region(cfg.region);
        }
        bus.input.set_concurrent_dpad(cfg.concurrent_dpad);
        bus.input.set_four_player(cfg.four_player);
        bus.input.connect_zapper(cfg.zapper);
        for (i, enabled) in cfg.channels_enabled.iter().enumerate() {
            match Channel::try_from(i) {
                Ok(channel) => bus.apu.set_channel_enabled(channel, *enabled),
                Err(apu::ParseChannelError) => tracing::error!("invalid APU channel: {i}"),
            }
        }
        for genie_code in cfg.genie_codes.iter().cloned() {
            bus.add_genie_code(genie_code);
        }
        let video = Video::with_filter(cfg.filter);
        Self {
            running: false,
            video,
            video_frame_stale: true,
            run_ahead: cfg.run_ahead,
            run_ahead_frames: None,
            clear_audio_on_clock: cfg.clear_audio_on_clock,
            loaded_rom: None,
            sram_dir: cfg.sram_dir.clone(),
            mapper_revisions: cfg.mapper_revisions,
            auto_detect_region: cfg.region.is_auto(),
            cycles_remaining: 0.0,
            frame_speed_step: 4,
            frame_accumulator: 0,
            frames_to_clock: 0,
            bus,
        }
    }

    /// Returns the path to the SRAM save file for a given ROM name, which is used to store
    /// battery-backed Cart RAM, or `None` when [`Config::sram_dir`] disables battery files.
    pub fn sram_path(&self, name: &str) -> Option<PathBuf> {
        self.sram_dir
            .as_ref()
            .map(|dir| dir.join(name).with_extension(Config::SRAM_EXTENSION))
    }

    /// Loads a ROM cartridge from anything implementing [`Read`], hard-resetting the console and
    /// restoring the cart's battery-backed RAM if it has any.
    ///
    /// `name` identifies the ROM, and is what [`ControlDeck::sram_path`] derives the save file name
    /// from. Use [`ControlDeck::load_rom_path`] to load from the filesystem.
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tetanes_core::prelude::*;
    ///
    /// let mut deck = ControlDeck::new();
    /// let rom = std::fs::read("some_awesome_game.nes")?;
    /// let loaded = deck.load_rom("some_awesome_game", &mut rom.as_slice())?;
    ///
    /// println!("{} ({:?})", loaded.name, loaded.region);
    /// assert!(deck.is_running());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// If the ROM header is malformed, its mapper is unimplemented, or the data can't be read, then
    /// an error is returned.
    pub fn load_rom<S: ToString, F: Read>(&mut self, name: S, rom: &mut F) -> Result<LoadedRom> {
        let name = name.to_string();
        self.unload_rom()?;
        // `Cart::from_rom` reports an unimplemented mapper as a nested `cart::Error`; unwrap it
        // back to this crate's own variant so callers see `unimplemented mapper \`69\`` rather
        // than a cart error wrapping a mapper error.
        let cart = Cart::from_rom(&name, rom, self.bus.ram_state).map_err(|err| match err {
            cart::Error::InvalidMapper(mapper::Error::Unimplemented(num)) => {
                Error::UnimplementedMapper(num)
            }
            err => Error::Cart(err),
        })?;
        let loaded_rom = LoadedRom {
            name: name.clone(),
            battery_backed: cart.battery_backed(),
            region: cart.region,
        };
        if self.auto_detect_region {
            self.bus.set_region(loaded_rom.region);
        }
        self.bus.load_cart(cart);
        self.loaded_rom = Some(loaded_rom.clone());
        self.update_mapper_revisions();
        self.reset(ResetKind::Hard);
        if let Some(path) = self.sram_path(&name)
            && let Err(err) = self.load_sram_path(path)
        {
            error!("failed to load SRAM: {err:?}");
        }
        Ok(loaded_rom)
    }

    /// Loads a ROM cartridge into memory from a path.
    ///
    /// # Errors
    ///
    /// If there is any issue loading the ROM, then an error is returned.
    pub fn load_rom_path(&mut self, path: impl AsRef<std::path::Path>) -> Result<LoadedRom> {
        use std::{fs::File, io::BufReader};

        let path = path.as_ref();
        let filename = fs::filename(path);
        info!("loading ROM: {filename}");
        File::open(path)
            .map_err(|err| Error::io(err, format!("failed to open rom {path:?}")))
            .and_then(|rom| self.load_rom(filename, &mut BufReader::new(rom)))
    }

    /// Unloads the currently loaded ROM and saves SRAM to disk if the Cart is battery-backed.
    ///
    /// Cheats are **not** cleared, here or in [`ControlDeck::load_rom`]. A [`Patch`] belongs to the
    /// session rather than to the machine - the same reason it survives a state restore, see
    /// [`ControlDeck::add_patch`] - so a caller that treats its cheat list as per-game has to call
    /// [`ControlDeck::clear_genie_codes`] itself when the cart changes. Left applied, a code
    /// substitutes at the same address in the next game, where it means something else entirely.
    ///
    /// Note that codes reach a deck by two routes with different lifetimes:
    /// [`Config::genie_codes`] is applied once when the deck is built, while
    /// [`ControlDeck::add_genie_code`] and [`ControlDeck::add_patch`] apply to the running console.
    /// Neither is re-applied by a load.
    ///
    /// # Errors
    ///
    /// If the loaded [`Cart`] is battery-backed and saving fails, then an error is returned.
    pub fn unload_rom(&mut self) -> Result<()> {
        if let Some(rom) = &self.loaded_rom
            && let Some(path) = self.sram_path(&rom.name)
            && let Err(err) = self.save_sram_path(path)
        {
            error!("failed to save SRAM: {err:?}");
        }
        self.loaded_rom = None;
        self.bus.unload_cart();
        self.running = false;
        Ok(())
    }

    /// Replaces the running console with a previously saved [`Bus`] state.
    ///
    /// # Errors
    ///
    /// If the state was not produced by the currently loaded cart, in which case the running
    /// console is left untouched.
    #[inline]
    pub fn load_bus(&mut self, bus: Bus) -> Result<()> {
        self.bus.load_state(bus)?;
        // Page tables are derived state and aren't serialized, so rebuild them from the restored
        // mapper registers.
        self.bus.rebuild_mapper_state();
        // A board revision is the player's choice, not the machine's - the state carries whichever
        // one was selected when it was recorded, and restoring that would change IRQ timing
        // underneath a running game.
        self.update_mapper_revisions();
        // The console this deck was showing is gone; anything cached from it describes a machine
        // that is no longer here.
        self.invalidate_frame();
        Ok(())
    }

    /// Set the [`MapperRevision`] to emulate for the any ROM loaded that uses this mapper.
    #[inline]
    pub const fn set_mapper_revision(&mut self, rev: MapperRevision) {
        self.mapper_revisions.set(rev);
        self.update_mapper_revisions();
    }

    /// Set the set of [`MapperRevisionsConfig`] to emulate for the any ROM loaded that uses this
    /// mapper.
    #[inline]
    pub const fn set_mapper_revisions(&mut self, revs: MapperRevisionsConfig) {
        self.mapper_revisions = revs;
        self.update_mapper_revisions();
    }

    /// Internal method to update the loaded ROM mapper revision when `mapper_revisions` is
    /// updated.
    const fn update_mapper_revisions(&mut self) {
        match &mut self.bus.mapper {
            Mapper::Txrom(mapper) => {
                mapper.set_revision(self.mapper_revisions.mmc3);
            }
            Mapper::Bf909x(mapper) => {
                mapper.set_revision(self.mapper_revisions.bf909);
            }
            // Remaining mappers all have more concrete detection via ROM headers
            Mapper::None(_)
            | Mapper::Nrom(_)
            | Mapper::Sxrom(_)
            | Mapper::Uxrom(_)
            | Mapper::Cnrom(_)
            | Mapper::Exrom(_)
            | Mapper::Axrom(_)
            | Mapper::Pxrom(_)
            | Mapper::Fxrom(_)
            | Mapper::ColorDreams(_)
            | Mapper::BandaiFCG(_)
            | Mapper::JalecoSs88006(_)
            | Mapper::Namco163(_)
            // VRC2/VRC4 needs no user-selected revision despite being nine boards over four mapper
            // numbers: a submapper names one outright, and without one the board decodes the union
            // of the candidates' address lines.
            | Mapper::Vrc24(_)
            | Mapper::Vrc6(_)
            | Mapper::TaitoTc0190(_)
            | Mapper::Bnrom(_)
            | Mapper::Nina001(_)
            | Mapper::Gxrom(_)
            | Mapper::SunsoftFme7(_)
            | Mapper::Nina003006(_)
            | Mapper::NesEvent(_)
            | Mapper::Fk23C(_) => (),
        }
    }

    /// Set whether concurrent D-Pad input is enabled which wasn't possible on the original NES.
    #[inline]
    pub fn set_concurrent_dpad(&mut self, enabled: bool) {
        self.bus.input.set_concurrent_dpad(enabled);
    }

    /// Set emulation RAM initialization state.
    #[inline]
    pub const fn set_ram_state(&mut self, ram_state: RamState) {
        self.bus.ram_state = ram_state;
    }

    /// Set the headless mode which can increase performance when the frame and audio outputs are
    /// not needed.
    #[inline]
    pub const fn set_headless_mode(&mut self, mode: HeadlessMode) {
        self.bus.ppu.skip_rendering = mode.contains(HeadlessMode::NO_VIDEO);
        self.bus.apu.skip_mixing = mode.contains(HeadlessMode::NO_AUDIO);
    }

    /// Set whether to emulate PPU warmup where writes to certain registers are ignored. Can result
    /// in some games not working correctly.
    ///
    /// See: <https://www.nesdev.org/wiki/PPU_power_up_state>
    #[inline]
    pub const fn set_emulate_ppu_warmup(&mut self, enabled: bool) {
        self.bus.ppu.emulate_warmup = enabled;
    }

    /// Attaches a debugger callback, to be executed any time the debugger conditions match.
    ///
    /// This replaces whatever was attached before.
    pub fn set_debugger(&mut self, debugger: Debugger) {
        self.bus.set_debugger(Some(debugger));
    }

    /// Detaches the debugger callback.
    pub fn clear_debugger(&mut self) {
        self.bus.set_debugger(None);
    }

    /// Start recording the last `capacity` executed program counters, or stop with `None`.
    ///
    /// See [`PcHistory`]. Executed instructions cannot be recovered afterwards by disassembling
    /// backwards without a known starting address to disassemble from from, so a debugger that
    /// wants to show them has to start recording before the instructions run.
    pub fn set_pc_history(&mut self, capacity: Option<usize>) {
        self.bus.pc_history = capacity.map(PcHistory::new);
    }

    /// The recorded program counters, if [`ControlDeck::set_pc_history`] asked for any.
    #[inline]
    #[must_use]
    pub const fn pc_history(&self) -> Option<&PcHistory> {
        self.bus.pc_history.as_ref()
    }

    /// Returns the name of the currently loaded ROM [`Cart`]. Returns `None` if no ROM is loaded.
    #[inline]
    #[must_use]
    pub const fn loaded_rom(&self) -> Option<&LoadedRom> {
        self.loaded_rom.as_ref()
    }

    /// Returns the auto-detected [`NesRegion`] for the loaded ROM. Returns `None` if no ROM is
    /// loaded.
    #[inline]
    #[must_use]
    pub fn cart_region(&self) -> Option<NesRegion> {
        self.loaded_rom.as_ref().map(|rom| rom.region)
    }

    /// Returns whether the loaded ROM is battery-backed. Returns `None` if no ROM is loaded.
    #[inline]
    #[must_use]
    pub fn cart_battery_backed(&self) -> Option<bool> {
        self.loaded_rom.as_ref().map(|rom| rom.battery_backed)
    }

    /// Returns the NES Work RAM.
    #[inline]
    #[must_use]
    pub fn wram(&self) -> &[u8; bus::size::WRAM] {
        self.bus.wram()
    }

    /// Returns the NES Work RAM, for a debugger or a frontend's cheat engine to write.
    #[inline]
    #[must_use]
    pub fn wram_mut(&mut self) -> &mut [u8; bus::size::WRAM] {
        self.bus.wram_mut()
    }

    /// The cart's whole battery as one slice: PRG-RAM, then any state the board keeps outside it.
    ///
    /// Empty for a cart without a battery. For all but a handful of boards this is PRG-RAM exactly,
    /// which is also what other emulators write to a `.srm`.
    #[must_use]
    pub fn sram(&mut self) -> &[u8] {
        if self.cart_battery_backed() == Some(true) {
            self.bus.sram()
        } else {
            &[]
        }
    }

    /// Write battery-backed Save RAM to `writer` (if the cartridge has any).
    ///
    /// # Errors
    ///
    /// If the writer fails, then an error is returned.
    pub fn save_sram(&mut self, writer: impl Write) -> Result<()> {
        if let Some(true) = self.cart_battery_backed() {
            info!("saving SRAM...");
            self.bus.save_sram(writer).map_err(Error::Sram)?;
        }
        Ok(())
    }

    /// Replaces battery-backed Save RAM with raw bytes, as [`ControlDeck::sram`] hands them out.
    ///
    /// For a frontend that owns the battery file itself. No-op for a cart without a battery.
    pub fn set_sram(&mut self, data: &[u8]) {
        if self.cart_battery_backed() == Some(true) {
            self.bus.set_sram(data);
        }
    }

    /// Read battery-backed Save RAM from `reader` (if the cartridge has any).
    ///
    /// # Errors
    ///
    /// If the reader fails, then an error is returned.
    pub fn load_sram(&mut self, reader: impl Read) -> Result<()> {
        if let Some(true) = self.cart_battery_backed() {
            info!("loading SRAM...");
            self.bus.load_sram(reader).map_err(Error::Sram)?;
        }
        Ok(())
    }

    /// Save battery-backed Save RAM to a file (if the cartridge has any).
    ///
    /// # Errors
    ///
    /// If the file path is invalid or fails to save, then an error is returned.
    pub fn save_sram_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        if let Some(true) = self.cart_battery_backed() {
            let path = path.as_ref();
            if path.is_dir() {
                return Err(Error::InvalidFilePath(path.to_path_buf()));
            }
            let path = path.with_extension(Config::SRAM_EXTENSION);
            let mut writer = sys_fs::writer_impl(&path).map_err(Error::Sram)?;
            self.save_sram(&mut writer)?;
            // The wasm backend commits to local storage on flush, so the save has not happened
            // until this returns.
            writer
                .flush()
                .map_err(|err| Error::Sram(fs::Error::io(err, "failed to save sram")))?;
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if the cartridge has any).
    ///
    /// A cart with no save file yet is not an error - it is a game that has not been played.
    ///
    /// # Errors
    ///
    /// If the file path is invalid or fails to load, then an error is returned.
    pub fn load_sram_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        if let Some(true) = self.cart_battery_backed() {
            let path = path.as_ref();
            if path.is_dir() {
                return Err(Error::InvalidFilePath(path.to_path_buf()));
            }
            let path = path.with_extension(Config::SRAM_EXTENSION);
            if path.is_file() {
                let reader = sys_fs::reader_impl(&path).map_err(Error::Sram)?;
                // A save that fails to load is not a save that may be discarded: the next unload
                // writes this same path back out, so an unreadable file would be overwritten by
                // whatever the console happens to hold.
                self.load_sram(reader)
                    .inspect_err(|err| fs::back_up_sram(&path, err))?;
            }
        }
        Ok(())
    }

    /// Write the current state of the console to `writer`.
    ///
    /// # Errors
    ///
    /// If no ROM is loaded or the writer fails, then an error is returned.
    pub fn save_state(&mut self, writer: impl Write) -> Result<()> {
        if self.loaded_rom().is_none() {
            return Err(Error::RomNotLoaded);
        };
        fs::save(writer, &self.bus).map_err(Error::SaveState)
    }

    /// Load the console with a state read from `reader`.
    ///
    /// # Errors
    ///
    /// If no ROM is loaded, the reader fails, or the state belongs to a different cart.
    pub fn load_state(&mut self, reader: impl Read) -> Result<()> {
        if self.loaded_rom().is_none() {
            return Err(Error::RomNotLoaded);
        };
        fs::load::<Bus>(reader)
            .map_err(Error::SaveState)
            .and_then(|mut bus| {
                bus.input.clear(); // Discard inputs from save states
                self.load_bus(bus)
            })
    }

    /// Bytes [`ControlDeck::serialize_state_into`] needs for the console as it stands.
    ///
    /// Fixed for the life of a loaded cart, so a frontend that must size a buffer once - libretro
    /// asks for a state size before it will ever call for a state - can ask here and keep the
    /// answer.
    ///
    /// # Errors
    ///
    /// If no ROM is loaded or the state cannot be encoded.
    pub fn serialized_state_len(&self) -> Result<usize> {
        if self.loaded_rom().is_none() {
            return Err(Error::RomNotLoaded);
        };
        let mut size = bincode::enc::write::SizeWriter::default();
        bincode::serde::encode_into_writer(&self.bus, &mut size, Self::STATE_CONFIG)
            .map_err(|err| Error::SaveState(fs::Error::SerializationFailed(err.to_string())))?;
        Ok(size.bytes_written)
    }

    /// Writes the console's state into `dst`, returning how many bytes it took.
    ///
    /// Uncompressed and unframed, for a frontend that has its own container and its own idea of
    /// when to write one. [`ControlDeck::save_state`] is the form that stamps a header and
    /// compresses, which is what a `.sav` file holds.
    ///
    /// # Errors
    ///
    /// If no ROM is loaded, or `dst` is shorter than [`ControlDeck::serialized_state_len`].
    pub fn serialize_state_into(&self, dst: &mut [u8]) -> Result<usize> {
        if self.loaded_rom().is_none() {
            return Err(Error::RomNotLoaded);
        };
        bincode::serde::encode_into_slice(&self.bus, dst, Self::STATE_CONFIG)
            .map_err(|err| Error::SaveState(fs::Error::SerializationFailed(err.to_string())))
    }

    /// Restores a console state written by [`ControlDeck::serialize_state_into`].
    ///
    /// Trailing bytes past the state are ignored, so a frontend may hand back a buffer it padded.
    ///
    /// # Errors
    ///
    /// If no ROM is loaded, the bytes are not a state, or the state belongs to a different cart.
    pub fn deserialize_state(&mut self, src: &[u8]) -> Result<()> {
        if self.loaded_rom().is_none() {
            return Err(Error::RomNotLoaded);
        };
        let (mut bus, _) = bincode::serde::decode_from_slice::<Bus, _>(src, Self::STATE_CONFIG)
            .map_err(|err| Error::SaveState(fs::Error::DeserializationFailed(err.to_string())))?;
        bus.input.clear(); // Discard inputs from save states
        self.load_bus(bus)
    }

    /// Save the current state of the console into a save file.
    ///
    /// # Errors
    ///
    /// If there is an issue saving the state, then an error is returned.
    pub fn save_state_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        if self.loaded_rom().is_none() {
            return Err(Error::RomNotLoaded);
        };
        fs::save_path(path, &self.bus).map_err(Error::SaveState)
    }

    /// Load the console with data saved from a save state, if it exists.
    ///
    /// # Errors
    ///
    /// If there is no save state at `path`, or there is an issue loading it.
    pub fn load_state_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        if self.loaded_rom().is_none() {
            return Err(Error::RomNotLoaded);
        };
        let path = path.as_ref();
        if !fs::exists(path) {
            return Err(Error::NoSaveStateFound);
        }
        let reader = sys_fs::reader_impl(path).map_err(Error::SaveState)?;
        self.load_state(reader)
    }

    /// Returns the frame to display as raw PPU pixels, one palette index per pixel, for callers
    /// doing their own color decoding.
    ///
    /// This is the frame the most recent clock produced. With run-ahead enabled that is a frame
    /// from *ahead* of where the console now sits, not the buffer
    /// [`Ppu::frame_buffer`](crate::ppu::Ppu::frame_buffer) currently holds.
    #[inline]
    #[must_use]
    pub fn frame_buffer_raw(&self) -> &[u16; ppu::size::FRAME] {
        match &self.run_ahead_frames {
            Some(frames) if frames.pending_valid => &frames.pending,
            _ => self.bus.ppu.frame_buffer(),
        }
    }

    /// Returns the frame to display as RGBA pixels, with the configured [`VideoFilter`] applied.
    ///
    /// The filter runs on the first call after a clock and the result is cached, so calling this
    /// more than once per frame is cheap.
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tetanes_core::prelude::*;
    ///
    /// let mut deck = ControlDeck::new();
    /// deck.load_rom_path("some_awesome_game.nes")?;
    /// deck.clock_frame()?;
    ///
    /// let frame = deck.frame_buffer(); // 256 * 240 pixels, 4 bytes each
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn frame_buffer(&mut self) -> &[u8; Frame::SIZE] {
        if !self.video_frame_stale {
            return self.video.frame.as_array();
        }
        self.video_frame_stale = false;
        // Matched inline rather than via `frame_buffer_raw` so the borrow checker can see that the
        // pixels and `self.video` are disjoint fields.
        match &self.run_ahead_frames {
            Some(frames) if frames.pending_valid => self
                .video
                .apply_filter(&frames.pending, frames.pending_frame_number),
            _ => self
                .video
                .apply_filter(self.bus.ppu.frame_buffer(), self.bus.ppu.frame_number()),
        }
    }

    /// Writes the frame to display into `buffer` as RGBA pixels, with the configured
    /// [`VideoFilter`] applied.
    ///
    /// Use this over [`ControlDeck::frame_buffer`] to render into a buffer you already own, such as
    /// one from a pool. [`Frame::as_array_mut`] gets you the argument from a [`Frame`].
    #[inline]
    pub fn frame_buffer_into(&self, buffer: &mut [u8; Frame::SIZE]) {
        match &self.run_ahead_frames {
            Some(frames) if frames.pending_valid => {
                self.video
                    .apply_filter_into(&frames.pending, frames.pending_frame_number, buffer)
            }
            _ => self.video.apply_filter_into(
                self.bus.ppu.frame_buffer(),
                self.bus.ppu.frame_number(),
                buffer,
            ),
        }
    }

    /// Returns the number of frames the console has rendered since power on.
    ///
    /// This tracks the console, so with run-ahead enabled it lags the frame
    /// [`ControlDeck::frame_buffer`] returns.
    #[inline(always)]
    #[must_use]
    pub const fn frame_number(&self) -> u32 {
        self.bus.ppu.frame_number()
    }

    /// Returns the audio samples produced by the most recent clock, at the configured sample rate.
    ///
    /// The samples are cleared at the start of each clock, so this is exactly one clock's worth of
    /// audio and nothing accumulates. Push them to your audio device before clocking again:
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tetanes_core::prelude::*;
    ///
    /// let mut deck = ControlDeck::new();
    /// deck.load_rom_path("some_awesome_game.nes")?;
    ///
    /// deck.clock_frame()?;
    /// let samples = deck.audio_samples(); // this frame's audio only
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// If you would rather clock several times before draining, set
    /// [`Config::clear_audio_on_clock`] to `false` and call [`ControlDeck::clear_audio_samples`]
    /// yourself - see [`ControlDeck::set_clear_audio_on_clock`].
    #[inline(always)]
    #[must_use]
    pub fn audio_samples(&self) -> &[f32] {
        self.bus.audio_samples()
    }

    /// Discards the audio samples produced so far.
    ///
    /// Only needed when [`Config::clear_audio_on_clock`] is `false`, or to drop audio outright -
    /// when it is `true` each clock does this for you.
    #[inline]
    pub fn clear_audio_samples(&mut self) {
        self.bus.clear_audio_samples();
    }

    /// Returns whether clocking discards the previous call's audio samples first.
    #[inline]
    #[must_use]
    pub const fn clear_audio_on_clock(&self) -> bool {
        self.clear_audio_on_clock
    }

    /// Sets whether clocking discards the previous call's audio samples first.
    ///
    /// `true`, the default, keeps [`ControlDeck::audio_samples`] to exactly what the most recent
    /// clock produced. Set it to `false` if you clock several times before draining audio, and call
    /// [`ControlDeck::clear_audio_samples`] yourself - otherwise the buffer grows without bound.
    #[inline]
    pub const fn set_clear_audio_on_clock(&mut self, enabled: bool) {
        self.clear_audio_on_clock = enabled;
    }

    /// Returns how many frames [`ControlDeck::clock_frame`] runs ahead of the console.
    #[inline]
    #[must_use]
    pub const fn run_ahead(&self) -> usize {
        self.run_ahead
    }

    /// Sets how many frames [`ControlDeck::clock_frame`] runs ahead of the console to hide input
    /// lag. `0`, the default, disables it.
    ///
    /// With run-ahead on, each [`ControlDeck::clock_frame`] clocks the current frame, snapshots the
    /// console, clocks `frames` more, and rewinds - so what you display is the console's state
    /// `frames` in the future and a button press appears to take effect that much sooner. The cost
    /// is clocking every frame `frames + 1` times, so it trades CPU for latency.
    ///
    /// Values above about 4 are rarely useful.
    ///
    /// Run-ahead only applies at 1x speed, whatever is set here: above it the extra frames cost
    /// more than the latency they hide, and below it a display frame does not always clock a frame
    /// to run ahead of. [`ControlDeck::run_ahead`] reports what was asked for;
    /// [`ControlDeck::clock_frame`] uses what applies.
    #[inline]
    pub const fn set_run_ahead(&mut self, frames: usize) {
        self.run_ahead = frames;
    }

    /// The run-ahead actually in force, which is none unless the speed is 1x.
    ///
    /// The rule lives here rather than in a frontend because it is a property of what run-ahead
    /// *is* - speculating past the current frame only makes sense when one display frame is one
    /// NES frame - and every frontend would otherwise have to know it.
    #[inline]
    const fn effective_run_ahead(&self) -> usize {
        if self.frame_speed_step == 4 {
            self.run_ahead
        } else {
            0
        }
    }

    /// CPU clock rate based on currently configured NES region.
    #[inline]
    #[must_use]
    pub const fn clock_rate(&self) -> f32 {
        self.bus.clock_rate()
    }

    /// Steps the control deck a single CPU instruction.
    ///
    /// Unlike the other `clock_*` methods this does not discard the previous audio samples, since
    /// one instruction is far shorter than one sample - the methods built on it clear once, up
    /// front, instead. It does invalidate the frame, because the PPU has moved and what
    /// [`ControlDeck::frame_buffer`] holds is no longer what the console shows.
    ///
    /// # Errors
    ///
    /// If the CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_instr(&mut self) -> Result<()> {
        self.invalidate_frame();
        self.step_instr()
    }

    /// One instruction, with no invalidation at all.
    //
    // What the `clock_*` methods loop on. They invalidate once up front, so doing it per
    // instruction would be both redundant and - since `clock_one_frame` runs this tens of
    // thousands of times a frame - on the hottest path in the emulator.
    #[inline(always)]
    fn step_instr(&mut self) -> Result<()> {
        self.clock();
        if self.cpu_corrupted() {
            self.running = false;
            return Err(Error::CpuCorrupted);
        }
        Ok(())
    }

    /// Steps the control deck the given number of seconds, returning the CPU cycles elapsed.
    ///
    /// # Errors
    ///
    /// If the CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_seconds(&mut self, seconds: f32) -> Result<u32> {
        self.begin_clock();
        self.cycles_remaining += self.clock_rate() * seconds;
        let mut total_cycles = 0;
        while self.cycles_remaining > 0.0 {
            let start_cycles = self.bus.cpu.cycle;
            self.step_instr()?;
            let cycles = self.bus.cpu.cycle - start_cycles;
            total_cycles += cycles;
            self.cycles_remaining -= cycles as f32;
        }
        Ok(total_cycles)
    }

    /// Invalidates the outputs of the previous clock.
    ///
    /// Called once at the top of each outer `clock_*` entry point - never per NES frame, because at
    /// 2x speed a single [`ControlDeck::clock_frame`] clocks two of them and both frames' audio has
    /// to survive.
    #[inline]
    fn begin_clock(&mut self) {
        self.invalidate_frame();
        if self.clear_audio_on_clock {
            self.bus.clear_audio_samples();
        }
    }

    /// Invalidates the frame the previous clock produced, leaving its audio alone.
    ///
    /// Separate from [`ControlDeck::begin_clock`] for the paths that move the console without
    /// being a clock of their own: stepping one instruction, and replacing the console wholesale
    /// in [`ControlDeck::load_bus`]. Rewind restores a state every display frame and relies on the
    /// audio it has already produced surviving, so those must not drain it.
    #[inline]
    fn invalidate_frame(&mut self) {
        self.video_frame_stale = true;
        if let Some(frames) = &mut self.run_ahead_frames {
            frames.pending_valid = false;
        }
    }

    /// Clocks a whole display frame, for the callers that cannot take the frames one at a time -
    /// the deprecated shims, which report one frame's output per call.
    fn clock_display_frame(&mut self) -> Result<()> {
        while self.clock_frame()? == Clocked::Continue {}
        Ok(())
    }

    /// Clocks exactly one NES frame, whatever the speed.
    fn clock_one_frame(&mut self) -> Result<()> {
        let frame = self.frame_number();
        while frame == self.frame_number() {
            self.step_instr()?;
        }
        self.bus.clock_sync();
        Ok(())
    }

    /// Clocks at most one NES frame, reporting what it did.
    ///
    /// This is the default way to drive the emulator. One *display* frame is however many NES
    /// frames [`ControlDeck::set_frame_speed`] asks for - two at 2x, one at 1x, and at 0.5x every
    /// other one is skipped - so this hands them out one at a time and says when the display frame
    /// is finished:
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tetanes_core::prelude::*;
    ///
    /// let mut deck = ControlDeck::new();
    /// deck.load_rom_path("some_awesome_game.nes")?;
    ///
    /// while deck.is_running() {
    ///     loop {
    ///         let clocked = deck.clock_frame()?;
    ///         // every NES frame the console runs, whatever the speed - where to snapshot for
    ///         // rewind, or to record video
    ///         if clocked != Clocked::Continue {
    ///             break;
    ///         }
    ///     }
    ///     let samples = deck.audio_samples(); // queue to an audio device
    ///     let frame = deck.frame_buffer();    // blit to the screen
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// At 1x - and at any speed, if you only ever call it once per display frame - a call is a
    /// frame, so `for _ in 0..300 { deck.clock_frame()?; }` clocks 300 of them.
    ///
    /// The outputs are per display frame rather than per call: [`ControlDeck::audio_samples`]
    /// covers every frame the loop clocked, and [`ControlDeck::frame_buffer`] holds the last one
    /// it rendered. Both are invalidated by the call that begins a display frame, not by each one.
    ///
    /// If run-ahead is enabled with [`ControlDeck::set_run_ahead`], the frame reported is from that
    /// many frames in the future and the console is rewound back afterwards. That happens on the
    /// call that finishes the display frame, and run-ahead is only ever active at 1x speed.
    ///
    /// # Errors
    ///
    /// If no ROM is loaded, or the CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_frame(&mut self) -> Result<Clocked> {
        if !self.running {
            return Err(Error::RomNotLoaded);
        }

        if self.frames_to_clock == 0 {
            self.begin_clock();
            // Speeds that aren't whole multiples owe a different number of frames each display
            // frame - 1.5x owes 1, 2, 1, 2, ... and 0.5x owes 0, 1, 0, 1, ... - so the leftover
            // quarters carry over rather than being rounded away.
            self.frame_accumulator += self.frame_speed_step;
            while self.frame_accumulator >= 4 {
                self.frame_accumulator -= 4;
                self.frames_to_clock += 1;
            }
            if self.frames_to_clock == 0 {
                self.clock_frame_run_ahead()?;
                return Ok(Clocked::Idle);
            }
        }

        self.clock_one_frame()?;
        self.frames_to_clock -= 1;
        if self.frames_to_clock > 0 {
            return Ok(Clocked::Continue);
        }
        self.clock_frame_run_ahead()?;
        Ok(Clocked::Complete)
    }

    /// Clocks `run_ahead` frames past the display frame just finished, reports the last of them,
    /// and rewinds the console back.
    ///
    /// Called at the end of a display frame rather than in place of one, because it is inherently
    /// a whole display frame's worth of work: [`ControlDeck::clock_frame`] hands out one NES frame
    /// at a time and this cannot be split that way.
    ///
    /// Run-ahead is only ever active at 1x, so "a display frame" and "a frame" are the same thing
    /// here and the frames below are clocked directly rather than through the speed accumulator.
    fn clock_frame_run_ahead(&mut self) -> Result<()> {
        if self.effective_run_ahead() == 0 {
            return Ok(());
        }

        let mut frames = self.run_ahead_frames.take().unwrap_or_default();

        // Park the frame just rendered and hand the PPU a recycled buffer to scribble over.
        std::mem::swap(&mut self.bus.ppu.frame.buffer, &mut frames.spare);

        // Snapshot the console. A plain copy, not a serialized state: run-ahead restores into the
        // same session one frame later, so a compact encoding buys nothing and measures 1.6-5.1x
        // slower than a clone (see benches/README.md). A copy also carries the page tables, so no
        // `rebuild_mapper_state` is needed on the way back.
        //
        // Rewind is the opposite trade and keeps the serialized form: it holds ~900 snapshots in
        // RAM at once, where a clone each would be hundreds of megabytes.
        //
        // It goes into the console the last run-ahead gave back rather than a fresh one: that
        // console has already run this cart, so its arena is the right size with the right ROM in
        // it, and only what a frame can change has to be copied.
        let mut saved = match frames.snapshot.take() {
            Some(mut saved) => {
                saved.snapshot_from(&self.bus);
                saved
            }
            None => Box::new(self.bus.clone()),
        };

        // Clock the intermediate frames, whose video is never seen. Restored rather than set back
        // to `false`, so this does not quietly turn rendering on for a headless deck.
        let skip_rendering = self.bus.ppu.skip_rendering;
        self.bus.ppu.skip_rendering = true;
        let result = (1..self.effective_run_ahead()).try_for_each(|_| self.clock_one_frame());
        self.bus.ppu.skip_rendering = skip_rendering;
        result?;

        // Their audio belongs to a timeline that is about to be rewound, so it is always dropped,
        // whatever `clear_audio_on_clock` says.
        self.bus.clear_audio_samples();

        // Clock the frame to actually display.
        self.clock_one_frame()?;

        // Park its pixels where `frame_buffer` can still reach them once the console has been
        // rewound past them.
        frames.pending_frame_number = self.bus.ppu.frame_number();
        std::mem::swap(&mut self.bus.ppu.frame.buffer, &mut frames.pending);
        frames.pending_valid = true;

        // Rewind, and give the console back the frame it had rendered. Through the same funnel
        // every other restore uses, so that the debugger and the cart's ROM are handled once.
        self.bus.swap_state(&mut saved)?;
        std::mem::swap(&mut self.bus.ppu.frame.buffer, &mut frames.spare);

        // `saved` is now the console the run-ahead frames left behind, which is next frame's
        // snapshot buffer.
        frames.snapshot = Some(saved);
        self.run_ahead_frames = Some(frames);
        Ok(())
    }

    /// Steps the control deck a single PPU scanline.
    ///
    /// # Errors
    ///
    /// If no ROM is loaded, or the CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_scanline(&mut self) -> Result<()> {
        if !self.running {
            return Err(Error::RomNotLoaded);
        }
        self.begin_clock();

        let current_scanline = self.bus.ppu.scanline;
        while current_scanline == self.bus.ppu.scanline {
            self.step_instr()?;
        }
        Ok(())
    }

    /// Returns whether the CPU is corrupted, which means it encountered an invalid/unhandled
    /// opcode and can't proceed executing the current ROM.
    #[inline]
    #[must_use]
    pub const fn cpu_corrupted(&self) -> bool {
        self.bus.cpu.corrupted
    }

    /// Returns the ControlDeck bus state which contains all components.
    ///
    /// The components are `pub` fields of [`Bus`], so this is the way to the
    /// [`Cpu`](crate::cpu::Cpu), [`Ppu`](crate::ppu::Ppu), [`Apu`], [`Mapper`],
    /// [`Memory`](crate::memory::Memory) and [`Input`](crate::input::Input):
    /// `deck.bus().ppu`, `deck.bus().memory`, and so on.
    ///
    /// NOTE: What is reachable here is internal and may change between releases - see the
    /// crate-level [stability](crate#stability) note.
    #[inline]
    pub const fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Returns a mutable reference to the ControlDeck bus state.
    ///
    /// NOTE: What is reachable here is internal and may change between releases - see the
    /// crate-level [stability](crate#stability) note.
    #[inline]
    pub const fn bus_mut(&mut self) -> &mut Bus {
        &mut self.bus
    }

    /// Returns the current four player mode.
    #[inline]
    pub const fn four_player(&self) -> FourPlayer {
        self.bus.input.four_player
    }

    /// Enable/Disable Four Score for 4-player controllers.
    #[inline]
    pub fn set_four_player(&mut self, four_player: FourPlayer) {
        self.bus.input.set_four_player(four_player);
    }

    /// Returns the current [`Joypad`] state for a given controller slot.
    #[inline]
    pub const fn joypad(&self, slot: Player) -> &Joypad {
        self.bus.input.joypad(slot)
    }

    /// Returns a mutable reference to the current [`Joypad`] state for a given controller slot.
    #[inline]
    pub const fn joypad_mut(&mut self, slot: Player) -> &mut Joypad {
        self.bus.input.joypad_mut(slot)
    }

    /// Returns whether the [`Zapper`](crate::input::Zapper) gun is connected.
    #[inline]
    pub const fn zapper_connected(&self) -> bool {
        self.bus.input.zapper.connected
    }

    /// Enable [`Zapper`](crate::input::Zapper) gun.
    #[inline]
    pub const fn connect_zapper(&mut self, enabled: bool) {
        self.bus.input.connect_zapper(enabled);
    }

    /// Returns the current [`Zapper`](crate::input::Zapper) aim position.
    #[inline]
    #[must_use]
    pub const fn zapper_pos(&self) -> (u16, u16) {
        let zapper = self.bus.input.zapper;
        (zapper.x, zapper.y)
    }

    /// Trigger [`Zapper`](crate::input::Zapper) gun.
    #[inline]
    pub fn trigger_zapper(&mut self) {
        self.bus.input.zapper.trigger();
    }

    /// Aim [`Zapper`](crate::input::Zapper) gun.
    #[inline]
    pub const fn aim_zapper(&mut self, x: u16, y: u16) {
        self.bus.input.zapper.aim(x, y);
    }

    /// Set the video filter for frame buffer output when calling [`ControlDeck::frame_buffer`].
    #[inline]
    pub const fn set_filter(&mut self, filter: VideoFilter) {
        self.video.filter = filter;
        // The cached frame was filtered with the old one.
        self.video_frame_stale = true;
    }

    /// Set the [`Apu`] sample rate.
    #[inline]
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.bus.apu.set_sample_rate(sample_rate);
    }

    /// Set the emulation speed.
    #[inline]
    pub fn set_frame_speed(&mut self, speed: f32) {
        self.frame_speed_step = (speed * 4.0) as u8;
        self.bus.apu.set_frame_speed(speed);
    }

    /// Stretch or squeeze the audio output rate by a small ratio, for dynamic rate control.
    ///
    /// A frontend that syncs video to its own clock and audio to the sound card's needs somewhere
    /// to put the difference between the two. This is that somewhere: see
    /// [`Apu::set_sample_ratio`] for what it costs and why frame timing is the wrong place.
    #[inline]
    pub fn set_audio_sample_ratio(&mut self, ratio: f32) {
        self.bus.apu.set_sample_ratio(ratio);
    }

    /// Add a NES Game Genie code.
    ///
    /// # Errors
    ///
    /// If the genie code is invalid, an error is returned.
    #[inline]
    pub fn add_genie_code(&mut self, genie_code: String) -> Result<()> {
        self.bus.add_genie_code(GenieCode::new(genie_code)?);
        Ok(())
    }

    /// Remove a NES Game Genie code.
    #[inline]
    pub fn remove_genie_code(&mut self, genie_code: &str) {
        self.bus.remove_genie_code(genie_code);
    }

    /// Remove all NES Game Genie codes.
    #[inline]
    pub fn clear_genie_codes(&mut self) {
        self.bus.clear_genie_codes();
    }

    /// Apply a [`Patch`], replacing any already at its address.
    ///
    /// A Game Genie code is one of these, and [`ControlDeck::add_genie_code`] is the way in for
    /// those. This is for a cheat the Game Genie cannot express - one below `$8000`, which is
    /// where a frontend's raw address/value cheats overwhelmingly point.
    #[inline]
    pub fn add_patch(&mut self, patch: Patch) {
        self.bus.patches.insert(patch);
    }

    /// Remove the patch at `addr`, if any.
    #[inline]
    pub fn remove_patch(&mut self, addr: u16) {
        self.bus.patches.remove(addr);
    }

    /// Every patch currently applied, in no particular order.
    #[inline]
    pub fn patches(&self) -> impl Iterator<Item = &Patch> {
        self.bus.patches.iter()
    }

    /// Returns whether a given [`Apu`] [`Channel`] is enabled.
    #[inline]
    #[must_use]
    pub const fn apu_channel_enabled(&self, channel: Channel) -> bool {
        self.bus.apu.channel_enabled(channel)
    }

    /// Returns whether a given [`Apu`] [`Channel`] is enabled.
    #[inline]
    #[must_use]
    #[deprecated(since = "0.15.0", note = "renamed to `apu_channel_enabled`")]
    pub const fn channel_enabled(&self, channel: Channel) -> bool {
        self.apu_channel_enabled(channel)
    }

    /// Enable or disable a given [`Apu`] [`Channel`].
    #[inline]
    pub const fn set_apu_channel_enabled(&mut self, channel: Channel, enabled: bool) {
        self.bus.apu.set_channel_enabled(channel, enabled);
    }

    /// Toggle a given [`Apu`] [`Channel`].
    #[inline]
    pub const fn toggle_apu_channel(&mut self, channel: Channel) {
        self.bus.apu.toggle_channel(channel);
    }

    /// Returns whether the control deck is currently running.
    #[inline]
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// Steps the control deck a single CPU instruction, without checking for CPU corruption.
    ///
    /// Prefer [`ControlDeck::clock_instr`], which reports an invalid opcode instead of leaving the
    /// console wedged.
    #[inline(always)]
    pub fn clock(&mut self) {
        self.bus.clock_instr()
    }

    /// Get the NES format for the emulation.
    pub const fn region(&self) -> NesRegion {
        self.bus.region
    }

    /// Set the NES format for the emulation.
    pub fn set_region(&mut self, region: NesRegion) {
        self.auto_detect_region = region.is_auto();
        if self.auto_detect_region {
            self.bus.set_region(self.cart_region().unwrap_or_default());
        } else {
            self.bus.set_region(region);
        }
    }

    /// Resets the console.
    pub fn reset(&mut self, kind: ResetKind) {
        self.bus.reset(kind);
        self.video_frame_stale = true;
        if let Some(frames) = &mut self.run_ahead_frames {
            frames.pending_valid = false;
        }
        if self.loaded_rom.is_some() {
            self.running = true;
        }
    }
}

/// Clocking methods superseded by [`ControlDeck::clock_frame`] plus the output accessors.
///
/// Each bundles advancing the emulation with retrieving its output, one variant per combination
/// of allocating-vs-copying, closure-vs-accessor and run-ahead-vs-not. [`ControlDeck::clock_frame`]
/// covers all of it: it honours [`ControlDeck::set_run_ahead`], and
/// [`ControlDeck::frame_buffer`] / [`ControlDeck::frame_buffer_into`] /
/// [`ControlDeck::audio_samples`] report what it produced.
#[allow(deprecated)]
impl ControlDeck {
    /// Steps the control deck an entire frame, calling `handle_output` with the `frame_buffer` and
    /// `audio_samples` for that frame.
    ///
    /// # Errors
    ///
    /// If the CPU encounters an invalid opcode, then an error is returned.
    #[deprecated(
        since = "0.15.0",
        note = "use `clock_frame` then `frame_buffer` and `audio_samples`"
    )]
    pub fn clock_frame_output<T>(
        &mut self,
        handle_output: impl FnOnce(&[u8], &[f32]) -> T,
    ) -> Result<T> {
        self.clock_display_frame()?;
        // Fills `self.video.frame`, so the two outputs can then be borrowed as disjoint fields.
        let _ = self.frame_buffer();
        Ok(handle_output(
            &self.video.frame[..],
            self.bus.audio_samples(),
        ))
    }

    /// Steps the control deck an entire frame, copying the `frame_buffer` and
    /// `audio_samples` for that frame into the provided buffers.
    ///
    /// Each buffer is filled as far as it goes; a longer one keeps its remaining contents.
    ///
    /// # Errors
    ///
    /// If the CPU encounters an invalid opcode, then an error is returned.
    #[deprecated(
        since = "0.15.0",
        note = "use `clock_frame` then `frame_buffer_into` and `audio_samples`"
    )]
    pub fn clock_frame_into(
        &mut self,
        frame_buffer: &mut [u8],
        audio_samples: &mut [f32],
    ) -> Result<()> {
        self.clock_display_frame()?;
        // This shim takes unsized slices, so it filters into `self.video.frame` and copies out;
        // `frame_buffer_into` requires a `&mut [u8; Frame::SIZE]`.
        let frame = self.frame_buffer();
        let len = frame.len().min(frame_buffer.len());
        frame_buffer[..len].copy_from_slice(&frame[..len]);
        let audio = self.bus.audio_samples();
        // Copy the shorter of the two rather than truncating to `audio_samples.len()`, which
        // panics whenever the caller's buffer is longer than the frame - and a caller cannot know
        // how many samples a frame produces.
        let len = audio.len().min(audio_samples.len());
        audio_samples[..len].copy_from_slice(&audio[..len]);
        Ok(())
    }

    /// Steps the control deck an entire frame with run-ahead frames to reduce input lag.
    ///
    /// # Errors
    ///
    /// If the CPU encounters an invalid opcode, then an error is returned.
    #[deprecated(
        since = "0.15.0",
        note = "use `set_run_ahead` then `clock_frame` and the output accessors"
    )]
    pub fn clock_frame_ahead<T>(
        &mut self,
        run_ahead: usize,
        handle_output: impl FnOnce(&[u8], &[f32]) -> T,
    ) -> Result<T> {
        let prev = std::mem::replace(&mut self.run_ahead, run_ahead);
        let res = self.clock_frame_output(handle_output);
        self.run_ahead = prev;
        res
    }

    /// Steps the control deck an entire frame with run-ahead frames to reduce input lag, copying
    /// the `frame_buffer` and `audio_samples` for that frame into the provided buffers.
    ///
    /// # Errors
    ///
    /// If the CPU encounters an invalid opcode, then an error is returned.
    #[deprecated(
        since = "0.15.0",
        note = "use `set_run_ahead` then `clock_frame` and the output accessors"
    )]
    pub fn clock_frame_ahead_into(
        &mut self,
        run_ahead: usize,
        frame_buffer: &mut [u8],
        audio_samples: &mut [f32],
    ) -> Result<()> {
        let prev = std::mem::replace(&mut self.run_ahead, run_ahead);
        let res = self.clock_frame_into(frame_buffer, audio_samples);
        self.run_ahead = prev;
        res
    }

    /// Steps the control deck the given number of seconds, calling `handle_audio` with the audio
    /// samples and `handle_frame` with the `frame_buffer` if a frame was completed.
    ///
    /// # Errors
    ///
    /// If the CPU encounters an invalid opcode, then an error is returned.
    #[deprecated(
        since = "0.15.0",
        note = "use `clock_seconds` then `frame_buffer` and `audio_samples`"
    )]
    pub fn clock_seconds_output(
        &mut self,
        seconds: f32,
        handle_audio: impl FnOnce(&[f32]),
        handle_frame: impl FnOnce(&[u8]),
    ) -> Result<()> {
        let frame = self.frame_number();
        self.clock_seconds(seconds)?;
        handle_audio(self.bus.audio_samples());
        if frame != self.frame_number() {
            handle_frame(self.frame_buffer());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::JoypadBtnState;
    use std::{
        fs::File,
        hash::{DefaultHasher, Hash, Hasher},
    };

    use crate::memory::Src;

    fn spritecans() -> ControlDeck {
        let mut deck = ControlDeck::with_config(Config {
            ram_state: RamState::AllZeros,
            ..Default::default()
        });
        let path = "test_roms/spritecans.nes";
        let mut rom = File::open(path).expect("test rom exists");
        deck.load_rom(path, &mut rom).expect("test rom loads");
        deck
    }

    /// Drains one display frame, returning how many NES frames it clocked.
    fn clock_display_frame(deck: &mut ControlDeck) -> usize {
        let mut clocked = 0;
        loop {
            match deck.clock_frame().expect("clocks") {
                Clocked::Continue => clocked += 1,
                Clocked::Complete => return clocked + 1,
                Clocked::Idle => return clocked,
            }
        }
    }

    /// Clock `frames` and return a hash of what ends up on screen.
    fn run(deck: &mut ControlDeck, frames: u32) -> u64 {
        for _ in 0..frames {
            // These all run at the default speed, where a call is a frame.
            let _ = deck.clock_frame().expect("clocks");
        }
        let mut hasher = DefaultHasher::new();
        deck.frame_buffer().hash(&mut hasher);
        hasher.finish()
    }

    /// A restored state has to resume bit-identically. Page tables are `#[serde(skip)]` derived
    /// state, so this only holds if `Bus::load_state` replays the mapper's registers through
    /// `Map::update_banks` - without it every page comes back unmapped and the machine reads
    /// zeroes.
    #[test]
    fn save_state_resumes_identically() {
        let mut deck = spritecans();
        run(&mut deck, 30);
        let mut state = Vec::new();
        deck.save_state(&mut state).expect("saves");
        let expected = run(&mut deck, 30);

        let mut restored = spritecans();
        run(&mut restored, 5); // somewhere else entirely
        restored.load_state(state.as_slice()).expect("loads");
        assert_eq!(run(&mut restored, 30), expected);
    }

    /// The unframed form has to resume identically too, and round-trip through a buffer sized from
    /// [`ControlDeck::serialized_state_len`] with room to spare - a frontend that pads is the case
    /// this exists for.
    #[test]
    fn a_serialized_state_resumes_identically() {
        let mut deck = spritecans();
        run(&mut deck, 30);

        let len = deck.serialized_state_len().expect("sizes");
        let mut state = vec![0xAA; len + 4096];
        assert_eq!(
            deck.serialize_state_into(&mut state).expect("serializes"),
            len,
            "the size asked for is the size written"
        );
        let expected = run(&mut deck, 30);

        let mut restored = spritecans();
        run(&mut restored, 5); // somewhere else entirely
        // Padding and all: trailing bytes past the state are not the state's problem.
        restored.deserialize_state(&state).expect("deserializes");
        assert_eq!(run(&mut restored, 30), expected);
    }

    /// A frontend asks for a state size once and reuses the answer for the life of the cart, so a
    /// state must never outgrow it. Nothing serialized in a [`Bus`] is variable-length today, and
    /// this is what says so when a future field is.
    #[test]
    fn a_serialized_state_is_the_same_size_every_frame() {
        let mut deck = spritecans();
        let expected = deck.serialized_state_len().expect("sizes");

        for frame in 0..300 {
            let _ = deck.clock_frame().expect("clocks");
            // Every joypad bit set, so input state cannot be what holds the size still.
            deck.joypad_mut(Player::One).buttons = JoypadBtnState::all();
            assert_eq!(
                deck.serialized_state_len().expect("sizes"),
                expected,
                "frame {frame}"
            );
        }
    }

    /// A buffer that cannot hold the state is an error, not a truncated state a later load would
    /// read back as garbage.
    #[test]
    fn serializing_into_too_small_a_buffer_fails() {
        let deck = spritecans();
        let len = deck.serialized_state_len().expect("sizes");
        let mut small = vec![0; len - 1];
        assert!(deck.serialize_state_into(&mut small).is_err());
    }

    /// Restoring a state must not restore the *player's* settings with it.
    ///
    /// The one that bites is emulation speed, because it is split in two: `frame_speed_step` here
    /// decides how many NES frames a `clock_frame` runs, while `Apu::speed` stretches the sample
    /// period, and only the second is inside `Bus`. So loading a state recorded during fast-forward
    /// must not leave the APU at 2x under a deck clocking at 1x: that halves the samples produced
    /// per frame, and a frontend that paces itself by waiting on a full audio queue then never
    /// waits at all. Rewinding past a fast-forward must not switch fast-forward back on either.
    #[test]
    fn a_restored_state_keeps_the_current_settings() {
        // Record a state with settings a player might have had at the time.
        let mut deck = spritecans();
        deck.set_frame_speed(2.0);
        deck.set_sample_rate(44_100.0);
        deck.set_apu_channel_enabled(Channel::Triangle, false);
        deck.add_genie_code("AAAAAA".to_string())
            .expect("valid code");
        run(&mut deck, 10);
        let mut state = Vec::new();
        deck.save_state(&mut state).expect("saves");

        // Now they are somewhere else entirely.
        let mut restored = spritecans();
        restored.set_frame_speed(1.0);
        restored.set_sample_rate(48_000.0);
        restored.set_apu_channel_enabled(Channel::Triangle, true);
        restored.set_apu_channel_enabled(Channel::Noise, false);
        restored.set_emulate_ppu_warmup(true);
        // Anything but `RamState::default()`, or the assertion below passes either way.
        restored.set_ram_state(RamState::AllOnes);
        restored
            .add_genie_code("ZEXPYGLA".to_string())
            .expect("valid code");
        let session_codes = restored.patches().map(|p| p.addr).collect::<Vec<_>>();
        run(&mut restored, 5);
        restored.load_state(state.as_slice()).expect("loads");

        assert_eq!(restored.bus().apu.speed, 1.0, "speed is the session's");
        assert_eq!(
            restored.bus().apu.sample_rate,
            48_000.0,
            "so is sample rate"
        );
        assert_eq!(
            restored.bus().apu.synth.samples_per_cycle(),
            crate::apu::band_limited::BandLimited::new(restored.bus().clock_rate(), 48_000.0, 1)
                .samples_per_cycle(),
            "and the synthesiser is tuned to them, or it produces samples at the wrong rate"
        );
        assert!(
            restored.apu_channel_enabled(Channel::Triangle),
            "a channel the player unmuted stays unmuted"
        );
        assert!(
            !restored.apu_channel_enabled(Channel::Noise),
            "and one they muted stays muted"
        );
        assert!(
            restored.bus().ppu.emulate_warmup,
            "warmup emulation is a setting, not machine state"
        );
        assert_eq!(
            restored.bus().ram_state,
            RamState::AllOnes,
            "`#[serde(skip)]` is not enough on its own: the restore is `*self = state`, so a \
             skipped field arrives as `Default` rather than keeping the running value"
        );
        assert_eq!(
            restored.patches().map(|p| p.addr).collect::<Vec<_>>(),
            session_codes,
            "the session's cheats survive, and the recorded state's do not come back with it"
        );
    }

    /// Run-ahead clocks the current frame, snapshots, runs ahead to produce the *displayed* frame,
    /// then rewinds to the snapshot - so afterwards the console must sit exactly where a single
    /// `clock_frame` would have left it, and carry on identically.
    ///
    /// This has no coverage otherwise, and the snapshot is a field-wise copy into the console the
    /// last one gave back rather than a serialized state, so nothing else would notice if it
    /// stopped being faithful.
    #[test]
    fn run_ahead_leaves_the_console_where_a_plain_frame_would() {
        for run_ahead in 1..=4 {
            let mut ahead = spritecans();
            run(&mut ahead, 30);
            ahead.set_run_ahead(run_ahead);
            let _ = ahead.clock_frame().expect("clocks ahead");
            ahead.set_run_ahead(0);

            let mut plain = spritecans();
            run(&mut plain, 30);
            let _ = plain.clock_frame().expect("clocks");

            assert_eq!(
                run(&mut ahead, 30),
                run(&mut plain, 30),
                "run_ahead {run_ahead} must resume identically"
            );
        }
    }

    /// The whole point of run-ahead is that you *display* the future frame, but the console is
    /// rewound past it before `clock_frame` returns - so the pixels have to be parked somewhere or
    /// they go with it, and `frame_buffer` has to hand back the parked ones rather than whatever the
    /// rewound console holds.
    #[test]
    fn run_ahead_reports_the_future_frame_not_the_rewound_one() {
        // What the screen should show is whatever a console clocked `run_ahead` frames further
        // along would show.
        for run_ahead in 1..=4 {
            let mut ahead = spritecans();
            run(&mut ahead, 30);
            ahead.set_run_ahead(run_ahead);
            let _ = ahead.clock_frame().expect("clocks ahead");

            let mut expected = spritecans();
            run(&mut expected, 30);
            for _ in 0..=run_ahead {
                let _ = expected.clock_frame().expect("clocks");
            }

            let mut hasher = DefaultHasher::new();
            ahead.frame_buffer().hash(&mut hasher);
            let displayed = hasher.finish();

            let mut hasher = DefaultHasher::new();
            expected.frame_buffer().hash(&mut hasher);

            assert_eq!(
                displayed,
                hasher.finish(),
                "run_ahead {run_ahead} must display the frame from {run_ahead} frames ahead"
            );

            // ...and `frame_buffer_into` has to agree with `frame_buffer`. Destination is a
            // `Frame`, not a zeroed `Vec`: the filters write RGB only, leaving the alpha byte
            // `Frame::new` pre-fills with 255.
            let mut into = Frame::new();
            ahead.frame_buffer_into(into.as_array_mut());
            assert_eq!(
                into.as_array(),
                ahead.frame_buffer(),
                "run_ahead {run_ahead}: frame_buffer_into must match frame_buffer"
            );
        }
    }

    /// Clocking drops the previous call's samples by default, so a caller that never reaches for
    /// `clear_audio_samples` still does not grow the buffer without bound.
    #[test]
    fn audio_samples_do_not_accumulate_across_frames() {
        let mut deck = spritecans();
        let _ = deck.clock_frame().expect("clocks");
        let one_frame = deck.audio_samples().len();
        assert!(one_frame > 0, "a frame should produce audio");

        // Per-frame counts wobble by a sample or two, because a frame is not a whole number of
        // output samples and the synthesiser carries the remainder into the next one - so this
        // bounds the buffer at a frame's worth rather than pinning it. What it rules out is frame
        // 60 holding 60 frames of audio.
        let mut max = 0;
        for _ in 0..60 {
            let _ = deck.clock_frame().expect("clocks");
            max = max.max(deck.audio_samples().len());
        }
        assert!(
            max < one_frame * 2,
            "samples accumulated across frames without a manual clear: \
             one frame is {one_frame}, peaked at {max}"
        );
    }

    /// Opting out lets the samples accumulate, for callers that clock several times before draining.
    #[test]
    fn audio_samples_accumulate_when_opted_out() {
        let mut deck = spritecans();
        deck.set_clear_audio_on_clock(false);
        let _ = deck.clock_frame().expect("clocks");
        let one_frame = deck.audio_samples().len();

        for _ in 0..9 {
            let _ = deck.clock_frame().expect("clocks");
        }
        assert!(
            deck.audio_samples().len() > one_frame * 9,
            "10 frames should have accumulated roughly 10 frames of audio"
        );

        deck.clear_audio_samples();
        assert_eq!(deck.audio_samples().len(), 0);
    }

    /// The clear happens once per *display* frame, not per NES frame - above 1x a display frame is
    /// several of them and every one's samples has to reach the caller.
    #[test]
    fn faster_than_realtime_reports_every_clocked_frames_audio() {
        let mut deck = spritecans();
        assert_eq!(clock_display_frame(&mut deck), 1);
        let one_frame = deck.audio_samples().len();

        deck.set_frame_speed(2.0);
        assert_eq!(clock_display_frame(&mut deck), 2);
        // `set_frame_speed` also halves the APU's sample period, so two frames at 2x speed produce
        // about as many samples as one at 1x - the point is that neither frame is dropped.
        assert!(
            deck.audio_samples().len() >= one_frame,
            "2x speed dropped one of the two frames it clocked"
        );
    }

    /// A display frame is however many NES frames the speed asks for, and `clock_frame` hands them
    /// out one at a time - so what it reports has to add up to the speed.
    ///
    /// Speeds that are not whole multiples owe a different number each display frame, which is why
    /// the leftover quarters carry rather than being rounded away.
    #[test]
    fn a_display_frame_clocks_what_the_speed_asks_for() {
        for (speed, expected) in [
            (1.0, [1, 1, 1, 1]),
            (2.0, [2, 2, 2, 2]),
            (4.0, [4, 4, 4, 4]),
            (1.5, [1, 2, 1, 2]),
            (0.5, [0, 1, 0, 1]),
        ] {
            // A fresh deck each time: the accumulator carries leftovers between display frames, so
            // a shared one would depend on the speed before it.
            let mut deck = spritecans();
            deck.set_frame_speed(speed);
            let clocked = [(); 4].map(|()| clock_display_frame(&mut deck));
            assert_eq!(clocked, expected, "at {speed}x");
        }
    }

    /// `frame_buffer` caches the filtered frame, so every path that moves the console has to say
    /// so. Stepping instructions is the one a debugger uses, and it can cross a frame boundary
    /// like any other clock.
    #[test]
    fn stepping_instructions_refreshes_the_frame() {
        let mut deck = spritecans();
        // Past the ROM's first few frames, which are still identical to each other and so cannot
        // tell a refreshed frame from a cached one.
        for _ in 0..10 {
            clock_display_frame(&mut deck);
        }
        let first = deck.frame_buffer().to_vec();

        let frame = deck.frame_number();
        while frame == deck.frame_number() {
            deck.clock_instr().expect("steps");
        }
        assert_ne!(
            first,
            deck.frame_buffer().to_vec(),
            "a whole frame was stepped, so the cached one is not the current one"
        );
    }

    /// A restored state replaces the console outright. Anything cached from the old one describes
    /// a machine that is no longer here, including the frame a caller is about to blit.
    #[test]
    fn loading_a_state_refreshes_the_frame() {
        let mut deck = spritecans();
        clock_display_frame(&mut deck);
        let early = deck.bus().clone();
        let early_frame = deck.frame_buffer().to_vec();

        for _ in 0..30 {
            clock_display_frame(&mut deck);
        }
        let late_frame = deck.frame_buffer().to_vec();
        assert_ne!(early_frame, late_frame, "the game has moved on");

        deck.load_bus(early).expect("restores");
        assert_eq!(
            deck.frame_buffer().to_vec(),
            early_frame,
            "reports the restored console, not the one it replaced"
        );
    }

    /// Which buttons are held is emulated state the game reads, so it travels with the state -
    /// but carrying the session's four-player setting across used to hard-reset the input on the
    /// way, which rewind does once per display frame.
    #[test]
    fn restoring_a_state_keeps_the_buttons_it_recorded() {
        use crate::input::JoypadBtn;

        let mut deck = spritecans();
        clock_display_frame(&mut deck);
        deck.joypad_mut(Player::One).set_button(JoypadBtn::A, true);
        let held = deck.bus().clone();

        deck.joypad_mut(Player::One).set_button(JoypadBtn::A, false);
        deck.load_bus(held).expect("restores");
        assert!(
            deck.joypad_mut(Player::One).button(JoypadBtn::A.into()),
            "the state recorded A held, so the restored console has it held"
        );
    }

    /// A board revision is the player's choice, not the machine's. A state recorded under another
    /// one must not put it back and change IRQ timing underneath a running game.
    #[test]
    fn restoring_a_state_keeps_the_selected_mapper_revision() {
        use crate::mapper::{Mapper, MapperRevision, Mmc3Revision};

        let mut deck = ControlDeck::with_config(Config {
            ram_state: RamState::AllZeros,
            ..Default::default()
        });
        let path = "test_roms/mapper/m004_txrom/rev_b.nes";
        let mut rom = File::open(path).expect("test rom exists");
        deck.load_rom(path, &mut rom).expect("test rom loads");

        let revision = |deck: &ControlDeck| match &deck.bus().mapper {
            Mapper::Txrom(board) => board.mmc3.revision,
            _ => panic!("expected an MMC3 board"),
        };

        deck.set_mapper_revision(MapperRevision::Mmc3(Mmc3Revision::A));
        let recorded_under_a = deck.bus().clone();
        assert_eq!(revision(&deck), Mmc3Revision::A);

        deck.set_mapper_revision(MapperRevision::Mmc3(Mmc3Revision::BC));
        deck.load_bus(recorded_under_a).expect("restores");
        assert_eq!(
            revision(&deck),
            Mmc3Revision::BC,
            "the state carries A, but BC is what the player has selected now"
        );
    }

    /// Battery-backed state is written and restored through the board, since what is backed
    /// varies: PRG-RAM for almost everything, plus internal sound RAM on Namco163, and an EEPROM
    /// instead of PRG-RAM on Bandai's Datach carts. Driven through `Bus` rather than
    /// `ControlDeck::save_sram`, which no-ops for a cart without a battery.
    #[test]
    fn sram_round_trips_through_the_board() {
        let mut deck = spritecans();
        deck.bus_mut().memory.region_mut(Src::PrgRam)[..4].copy_from_slice(&[1, 2, 3, 4]);
        let mut sram = Vec::new();
        deck.bus_mut().save_sram(&mut sram).expect("saves");

        let mut restored = spritecans();
        restored
            .bus_mut()
            .load_sram(sram.as_slice())
            .expect("loads");
        assert_eq!(
            &restored.bus().memory.region_ref(Src::PrgRam)[..4],
            &[1, 2, 3, 4]
        );
    }
}
