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
//!     deck.clock_frame()?;
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

use crate::{
    apu::{self, Apu, Channel},
    bus::{self, Bus},
    cart::{self, Cart},
    common::{NesRegion, ResetKind},
    cpu::Cpu,
    debug::Debugger,
    fs,
    genie::{self, GenieCode},
    input::{FourPlayer, Joypad, Player},
    mapper::{self, Bf909Revision, Mapper, MapperRevision, Mmc3Revision},
    memory::RamState,
    ppu::{self, Ppu},
    video::{Frame, Video, VideoFilter},
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use thiserror::Error;
use tracing::{error, info};

/// Result returned from [`ControlDeck`] methods.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that [`ControlDeck`] can return.
#[derive(Error, Debug)]
#[must_use]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
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
    /// Data directory for storing battery-backed RAM.
    pub data_dir: PathBuf,
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

    /// Returns the directory used to store battery-backed Cart RAM.
    #[inline]
    #[must_use]
    pub fn sram_dir(&self) -> PathBuf {
        self.data_dir.join(Self::SRAM_DIR)
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
            data_dir: Self::default_data_dir(),
            mapper_revisions: MapperRevisionsConfig::default(),
            emulate_ppu_warmup: false,
            run_ahead: 0,
            clear_audio_on_clock: true,
        }
    }
}

/// Represents a loaded ROM [`Cart`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRom {
    /// Name of ROM.
    pub name: String,
    /// Whether the loaded Cart is battery-backed.
    pub battery_backed: bool,
    /// Auto-detected of the loaded Cart.
    pub region: NesRegion,
}

/// Frame buffers [`ControlDeck`] recycles for run-ahead.
///
/// Run-ahead rewinds the console *past* the frame it wants to display, which would take that
/// frame's pixels with it - so `pending` parks them where [`ControlDeck::frame_buffer`] can still
/// reach them afterwards. `spare` holds the console's real frame meanwhile, and comes back on
/// restore.
///
/// They exist to be swapped rather than allocated: [`Buffer`](crate::ppu::frame::Buffer)'s
/// `Default` allocates and zeroes 120 KiB, which this path would otherwise pay on every frame.
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
    /// Directory for storing battery-backed Cart RAM if a ROM is loaded.
    sram_dir: PathBuf,
    /// Mapper revisions to emulate for any ROM loaded that matches the given mappers.
    mapper_revisions: MapperRevisionsConfig,
    /// Whether to auto-detect the region based on the loaded Cart.
    auto_detect_region: bool,
    /// Remaining CPU cycles to execute used to clock a given number of seconds.
    cycles_remaining: f32,
    /// Emulated frame speed step ranging from 1 (0.25 speed) to 8 (2.0).
    frame_speed_step: u16,
    /// Accumulated frame speed to account for slower 1x speeds.
    frame_accumulator: u16,
    /// The console: CPU, PPU, APU, input and the cart's board.
    bus: Bus,
}

impl Default for ControlDeck {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlDeck {
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
    /// let deck = ControlDeck::with_config(Config {
    ///     headless_mode: HeadlessMode::NO_AUDIO | HeadlessMode::NO_VIDEO,
    ///     ram_state: RamState::AllZeros,
    ///     ..Default::default()
    /// });
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
            sram_dir: cfg.sram_dir(),
            mapper_revisions: cfg.mapper_revisions,
            auto_detect_region: cfg.region.is_auto(),
            cycles_remaining: 0.0,
            frame_speed_step: 4,
            frame_accumulator: 0,
            bus,
        }
    }

    /// Returns the path to the SRAM save file for a given ROM name, which is used to store
    /// battery-backed Cart RAM.
    pub fn sram_path(&self, name: &str) -> PathBuf {
        self.sram_dir
            .join(name)
            .with_extension(Config::SRAM_EXTENSION)
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
        // `Cart::from_rom` now rejects an unimplemented mapper itself rather than handing back a
        // `Mapper::none()` that reads as open bus; unwrap it back to this crate's own variant so
        // callers keep getting `unimplemented mapper \`69\`` rather than a nested cart error.
        let cart = Cart::from_rom(&name, rom, self.bus.ram_state).map_err(|err| match err {
            cart::Error::InvalidMapper(mapper::Error::Unimplemented(num)) => {
                Error::UnimplementedMapper(num)
            }
            err => Error::Cart(err),
        })?;
        let loaded_rom = LoadedRom {
            name: name.clone(),
            battery_backed: cart.battery_backed(),
            region: cart.region(),
        };
        if self.auto_detect_region {
            self.bus.set_region(loaded_rom.region);
        }
        self.bus.load_cart(cart);
        self.loaded_rom = Some(loaded_rom.clone());
        self.update_mapper_revisions();
        self.reset(ResetKind::Hard);
        let sram_dir = self.sram_path(&name);
        if let Err(err) = self.load_sram(sram_dir) {
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
    /// # Errors
    ///
    /// If the loaded [`Cart`] is battery-backed and saving fails, then an error is returned.
    pub fn unload_rom(&mut self) -> Result<()> {
        if let Some(rom) = &self.loaded_rom {
            let sram_dir = self.sram_path(&rom.name);
            if let Err(err) = self.save_sram(sram_dir) {
                error!("failed to save SRAM: {err:?}");
            }
        }
        self.loaded_rom = None;
        self.bus.unload_cart();
        self.running = false;
        Ok(())
    }

    /// Replaces the running console with a previously saved [`Cpu`] state.
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
            | Mapper::Vrc6(_)
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

    /// Adds a debugger callback to be executed any time the debugger conditions match.
    ///
    /// The callback is handed the whole [`Bus`], so one hook serves a CPU, PPU or APU debugger
    /// alike; see [`Debugger`].
    pub fn add_debugger(&mut self, debugger: Debugger) {
        self.bus.set_debugger(debugger);
    }

    /// Removes the debugger callback.
    pub fn remove_debugger(&mut self, _debugger: Debugger) {
        self.bus.set_debugger(Debugger::default());
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

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    ///
    /// # Errors
    ///
    /// If the file path is invalid or fails to save, then an error is returned.
    pub fn save_sram(&self, path: impl AsRef<Path>) -> Result<()> {
        if let Some(true) = self.cart_battery_backed() {
            let path = path.as_ref();
            if path.is_dir() {
                return Err(Error::InvalidFilePath(path.to_path_buf()));
            }

            info!("saving SRAM...");
            self.bus
                .save_sram(path.with_extension(Config::SRAM_EXTENSION))
                .map_err(Error::Sram)?;
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    ///
    /// # Errors
    ///
    /// If the file path is invalid or fails to load, then an error is returned.
    pub fn load_sram(&mut self, path: impl AsRef<Path>) -> Result<()> {
        if let Some(true) = self.cart_battery_backed() {
            let path = path.as_ref();
            if path.is_dir() {
                return Err(Error::InvalidFilePath(path.to_path_buf()));
            }
            if path.is_file() {
                info!("loading SRAM...");
                self.bus
                    .load_sram(path.with_extension(Config::SRAM_EXTENSION))
                    .map_err(Error::Sram)?;
            }
        }
        Ok(())
    }

    /// Save the current state of the console into a save file.
    ///
    /// # Errors
    ///
    /// If there is an issue saving the state, then an error is returned.
    pub fn save_state(&mut self, path: impl AsRef<Path>) -> Result<()> {
        if self.loaded_rom().is_none() {
            return Err(Error::RomNotLoaded);
        };
        let path = path.as_ref();
        fs::save(path, &self.bus).map_err(Error::SaveState)
    }

    /// Load the console with data saved from a save state, if it exists.
    ///
    /// # Errors
    ///
    /// If there is an issue loading the save state, then an error is returned.
    pub fn load_state(&mut self, path: impl AsRef<Path>) -> Result<()> {
        if self.loaded_rom().is_none() {
            return Err(Error::RomNotLoaded);
        };
        let path = path.as_ref();
        if fs::exists(path) {
            fs::load::<Bus>(path)
                .map_err(Error::SaveState)
                .and_then(|mut bus| {
                    bus.input.clear(); // Discard inputs from save states
                    self.load_bus(bus)
                })
        } else {
            Err(Error::NoSaveStateFound)
        }
    }

    /// Returns the frame to display as raw PPU pixels, one palette index per pixel, for callers
    /// doing their own color decoding.
    ///
    /// This is the frame the most recent clock produced. With run-ahead enabled that is a frame
    /// from *ahead* of where the console now sits, not [`ControlDeck::ppu`]'s current buffer.
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
            _ => self.video.apply_filter(
                self.bus.ppu.frame_buffer(),
                self.bus.ppu.frame_number(),
            ),
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
    /// Values above about 4 are rarely useful, and it should be turned off above 1x emulation
    /// speed, where the extra frames cost more than the latency they hide.
    #[inline]
    pub const fn set_run_ahead(&mut self, frames: usize) {
        self.run_ahead = frames;
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
    /// front, instead.
    ///
    /// # Errors
    ///
    /// If the CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_instr(&mut self) -> Result<()> {
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
            self.clock_instr()?;
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
        self.video_frame_stale = true;
        if let Some(frames) = &mut self.run_ahead_frames {
            frames.pending_valid = false;
        }
        if self.clear_audio_on_clock {
            self.bus.clear_audio_samples();
        }
    }

    /// Clocks whole NES frames, without any of the per-call bookkeeping `begin_clock` does - so
    /// run-ahead can drive it repeatedly within one [`ControlDeck::clock_frame`].
    fn clock_frames(&mut self) -> Result<()> {
        // Frames that aren't multiples of the default render 1 more/less frames
        // every other frame
        // e.g. a speed of 1.5 will clock # of frames: 1, 2, 1, 2, 1, 2, 1, 2, ...
        // A speed of 0.5 will clock 0, 1, 0, 1, 0, 1, 0, 1, 0, ...
        self.frame_accumulator += self.frame_speed_step;
        let mut frames_to_clock = 0;
        while self.frame_accumulator >= 4 {
            self.frame_accumulator -= 4;
            frames_to_clock += 1;
        }

        for _ in 0..frames_to_clock {
            let frame = self.frame_number();
            while frame == self.frame_number() {
                self.clock_instr()?;
            }
            self.bus.clock_sync();
        }

        Ok(())
    }

    /// Steps the control deck an entire frame.
    ///
    /// This is the default way to drive the emulator. Afterwards, [`ControlDeck::frame_buffer`]
    /// holds the frame to display and [`ControlDeck::audio_samples`] the audio to play:
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tetanes_core::prelude::*;
    ///
    /// let mut deck = ControlDeck::new();
    /// deck.load_rom_path("some_awesome_game.nes")?;
    ///
    /// while deck.is_running() {
    ///     deck.clock_frame()?;
    ///     let samples = deck.audio_samples(); // queue to an audio device
    ///     let frame = deck.frame_buffer();    // blit to the screen
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// How many NES frames this clocks depends on [`ControlDeck::set_frame_speed`]: at 2x speed it
    /// clocks two, at 0.5x it alternates between none and one. Either way the outputs above cover
    /// everything it clocked.
    ///
    /// If run-ahead is enabled with [`ControlDeck::set_run_ahead`], the frame reported is from that
    /// many frames in the future and the console is rewound back afterwards.
    ///
    /// # Errors
    ///
    /// If no ROM is loaded, or the CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_frame(&mut self) -> Result<()> {
        if !self.running {
            return Err(Error::RomNotLoaded);
        }
        self.begin_clock();
        if self.run_ahead == 0 {
            return self.clock_frames();
        }
        self.clock_frame_run_ahead()
    }

    /// Clocks a frame, then `run_ahead` more, reports the last of them, and rewinds the console
    /// back to the first.
    ///
    /// Split out of [`ControlDeck::clock_frame`] so the common `run_ahead == 0` path stays a
    /// straight call to `clock_frames`.
    fn clock_frame_run_ahead(&mut self) -> Result<()> {
        // Clock the frame the console is really on.
        self.clock_frames()?;

        let mut frames = self.run_ahead_frames.take().unwrap_or_default();

        // Park the frame just rendered and hand the PPU a recycled buffer to scribble over.
        std::mem::swap(&mut self.bus.ppu.frame.buffer, &mut frames.spare);

        // Snapshot the console. A plain clone, not a serialized state: run-ahead restores into
        // the same session one frame later, so a compact encoding buys nothing and measures
        // 1.6-5.1x slower than the clone (see benches/README.md). The clone also carries the page
        // tables, so no `rebuild_mapper_state` is needed on the way back.
        //
        // Rewind is the opposite trade and keeps the serialized form: it holds ~900 snapshots in
        // RAM at once, where a clone each would be hundreds of megabytes.
        let saved = self.bus.clone();

        // Clock the intermediate frames, whose video is never seen. Restored rather than set back
        // to `false`, so this does not quietly turn rendering on for a headless deck.
        let skip_rendering = self.bus.ppu.skip_rendering;
        self.bus.ppu.skip_rendering = true;
        let result = (1..self.run_ahead).try_for_each(|_| self.clock_frames());
        self.bus.ppu.skip_rendering = skip_rendering;
        result?;

        // Their audio belongs to a timeline that is about to be rewound, so it is always dropped,
        // whatever `clear_audio_on_clock` says.
        self.bus.clear_audio_samples();

        // Clock the frame to actually display.
        self.clock_frames()?;

        // Park its pixels where `frame_buffer` can still reach them once the console has been
        // rewound past them.
        frames.pending_frame_number = self.bus.ppu.frame_number();
        std::mem::swap(&mut self.bus.ppu.frame.buffer, &mut frames.pending);
        frames.pending_valid = true;

        // Rewind, and give the console back the frame it had rendered.
        self.bus = saved;
        std::mem::swap(&mut self.bus.ppu.frame.buffer, &mut frames.spare);

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
            self.clock_instr()?;
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

    /// Returns the current [`Cpu`] registers.
    ///
    /// Only the register file; what the CPU *does* is an `impl Bus`, so driving it goes through
    /// [`ControlDeck::bus_mut`].
    #[inline]
    pub const fn cpu(&self) -> &Cpu {
        &self.bus.cpu
    }

    /// Returns a mutable reference to the current [`Cpu`] registers.
    #[inline]
    pub const fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.bus.cpu
    }

    /// Returns the current [`Ppu`] state.
    #[inline]
    pub const fn ppu(&self) -> &Ppu {
        &self.bus.ppu
    }

    /// Returns a mutable reference to the current [`Ppu`] state.
    #[inline]
    pub const fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.bus.ppu
    }

    /// Returns the console - every component, and the whole of the emulated state.
    #[inline]
    pub const fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Returns a mutable reference to the console.
    #[inline]
    pub const fn bus_mut(&mut self) -> &mut Bus {
        &mut self.bus
    }

    /// Returns the current [`Apu`] state.
    #[inline]
    pub const fn apu(&self) -> &Apu {
        &self.bus.apu
    }

    /// Returns a mutable reference to the current [`Apu`] state.
    #[inline]
    pub const fn apu_mut(&mut self) -> &mut Apu {
        &mut self.bus.apu
    }

    /// Returns the current [`Mapper`] state.
    #[inline]
    pub const fn mapper(&self) -> &Mapper {
        &self.bus.mapper
    }

    /// Returns a mutable reference to the current [`Mapper`] state.
    #[inline]
    pub const fn mapper_mut(&mut self) -> &mut Mapper {
        &mut self.bus.mapper
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
        (zapper.x(), zapper.y())
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
        self.frame_speed_step = (speed * 4.0) as u16;
        self.bus.apu.set_frame_speed(speed);
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
        self.bus.region()
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
/// They bundled advancing the emulation with retrieving its output, in one variant per combination
/// of allocating-vs-copying, closure-vs-accessor and run-ahead-vs-not. [`ControlDeck::clock_frame`]
/// now covers all of it: it honours [`ControlDeck::set_run_ahead`], and
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
        self.clock_frame()?;
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
        self.clock_frame()?;
        // This shim keeps taking unsized slices, so it filters into `self.video.frame` and copies
        // out. `frame_buffer_into` now requires a `&mut [u8; Frame::SIZE]`.
        let frame = self.frame_buffer();
        let len = frame.len().min(frame_buffer.len());
        frame_buffer[..len].copy_from_slice(&frame[..len]);
        let audio = self.bus.audio_samples();
        // The original truncated to `audio_samples.len()`, which panicked whenever the caller's
        // buffer was longer than the frame - and a caller cannot know how many samples a frame
        // produces.
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

    /// Clock `frames` and return a hash of what ends up on screen.
    fn run(deck: &mut ControlDeck, frames: u32) -> u64 {
        for _ in 0..frames {
            deck.clock_frame().expect("clocks");
        }
        let mut hasher = DefaultHasher::new();
        deck.frame_buffer().hash(&mut hasher);
        hasher.finish()
    }

    /// A restored state has to resume bit-identically. Page tables are `#[serde(skip)]` derived
    /// state, so this only holds if `load_cpu` replays the mapper's registers through `Map::update_banks` -
    /// without it every page comes back unmapped and the machine reads zeroes.
    #[test]
    fn save_state_resumes_identically() {
        let path = std::env::temp_dir().join("tetanes-save-state-resumes-identically.sav");
        let _ = std::fs::remove_file(&path);

        let mut deck = spritecans();
        run(&mut deck, 30);
        deck.save_state(&path).expect("saves");
        let expected = run(&mut deck, 30);

        let mut restored = spritecans();
        run(&mut restored, 5); // somewhere else entirely
        restored.load_state(&path).expect("loads");
        assert_eq!(run(&mut restored, 30), expected);

        std::fs::remove_file(&path).expect("cleans up");
    }

    /// Run-ahead clocks the current frame, snapshots, runs ahead to produce the *displayed* frame,
    /// then rewinds to the snapshot - so afterwards the console must sit exactly where a single
    /// `clock_frame` would have left it, and carry on identically.
    ///
    /// This has no coverage otherwise, and the snapshot is a plain `Cpu::clone` rather than a
    /// serialized state, so nothing else would notice if the restore stopped being faithful.
    #[test]
    fn run_ahead_leaves_the_console_where_a_plain_frame_would() {
        for run_ahead in 1..=4 {
            let mut ahead = spritecans();
            run(&mut ahead, 30);
            ahead.set_run_ahead(run_ahead);
            ahead.clock_frame().expect("clocks ahead");
            ahead.set_run_ahead(0);

            let mut plain = spritecans();
            run(&mut plain, 30);
            plain.clock_frame().expect("clocks");

            assert_eq!(
                run(&mut ahead, 30),
                run(&mut plain, 30),
                "run_ahead {run_ahead} must resume identically"
            );
        }
    }

    /// The whole point of run-ahead is that you *display* the future frame, but the console is
    /// rewound past it before `clock_frame` returns - so the pixels have to be parked somewhere or
    /// they go with it. Before this was fixed, `frame_buffer` handed back the pre-run-ahead frame
    /// and the only way to see the right one was the old `clock_frame_ahead` closure.
    #[test]
    fn run_ahead_reports_the_future_frame_not_the_rewound_one() {
        // What the screen should show is whatever a console clocked `run_ahead` frames further
        // along would show.
        for run_ahead in 1..=4 {
            let mut ahead = spritecans();
            run(&mut ahead, 30);
            ahead.set_run_ahead(run_ahead);
            ahead.clock_frame().expect("clocks ahead");

            let mut expected = spritecans();
            run(&mut expected, 30);
            for _ in 0..=run_ahead {
                expected.clock_frame().expect("clocks");
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

    /// Audio used to accumulate until the caller remembered `clear_audio_samples`, which made
    /// forgetting it an unbounded leak. Clocking now drops the previous call's samples by default.
    #[test]
    fn audio_samples_do_not_accumulate_across_frames() {
        let mut deck = spritecans();
        deck.clock_frame().expect("clocks");
        let one_frame = deck.audio_samples().len();
        assert!(one_frame > 0, "a frame should produce audio");

        // Per-frame counts wobble by a sample or two, because `Apu::sample_counter` carries across
        // frames - so this bounds the buffer at a frame's worth rather than pinning it. What it
        // rules out is the old behavior, where frame 60 would hold 60 frames of audio.
        let mut max = 0;
        for _ in 0..60 {
            deck.clock_frame().expect("clocks");
            max = max.max(deck.audio_samples().len());
        }
        assert!(
            max < one_frame * 2,
            "samples accumulated across frames without a manual clear: \
             one frame is {one_frame}, peaked at {max}"
        );
    }

    /// Opting out restores the old behavior for callers who clock several times before draining.
    #[test]
    fn audio_samples_accumulate_when_opted_out() {
        let mut deck = spritecans();
        deck.set_clear_audio_on_clock(false);
        deck.clock_frame().expect("clocks");
        let one_frame = deck.audio_samples().len();

        for _ in 0..9 {
            deck.clock_frame().expect("clocks");
        }
        assert!(
            deck.audio_samples().len() > one_frame * 9,
            "10 frames should have accumulated roughly 10 frames of audio"
        );

        deck.clear_audio_samples();
        assert_eq!(deck.audio_samples().len(), 0);
    }

    /// The clear happens once per `clock_frame`, not per NES frame - above 1x speed one call clocks
    /// several and every one of their samples has to reach the caller.
    #[test]
    fn faster_than_realtime_reports_every_clocked_frames_audio() {
        let mut deck = spritecans();
        deck.clock_frame().expect("clocks");
        let one_frame = deck.audio_samples().len();

        deck.set_frame_speed(2.0);
        deck.clock_frame().expect("clocks");
        // `set_frame_speed` also halves the APU's sample period, so two frames at 2x speed produce
        // about as many samples as one at 1x - the point is that neither frame is dropped.
        assert!(
            deck.audio_samples().len() >= one_frame,
            "2x speed dropped one of the two frames it clocked"
        );
    }

    /// Battery-backed state is written and restored through the board, since what is backed
    /// varies: PRG-RAM for almost everything, plus internal sound RAM on Namco163, and an EEPROM
    /// instead of PRG-RAM on Bandai's Datach carts. Driven through `Bus` rather than
    /// `ControlDeck::save_sram`, which no-ops for a cart without a battery.
    #[test]
    fn sram_round_trips_through_the_board() {
        let path = std::env::temp_dir().join("tetanes-sram-round-trips.sram");
        let _ = std::fs::remove_file(&path);

        let mut deck = spritecans();
        deck.bus_mut().memory.region_mut(Src::PrgRam)[..4].copy_from_slice(&[1, 2, 3, 4]);
        deck.bus().save_sram(&path).expect("saves");

        let mut restored = spritecans();
        restored.bus_mut().load_sram(&path).expect("loads");
        assert_eq!(
            &restored.bus().memory.region_ref(Src::PrgRam)[..4],
            &[1, 2, 3, 4]
        );

        std::fs::remove_file(&path).expect("cleans up");
    }
}
