//! Control Deck implementation. The primary entry-point for emulating the NES.

use crate::{
    apu::{Apu, Channel},
    bus::Bus,
    cart::{self, Cart},
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sram},
    cpu::Cpu,
    fs,
    genie::{self, GenieCode},
    input::{FourPlayer, Joypad, Player},
    mapper::{Bf909Revision, Mapper, MapperRevision, Mmc3Revision},
    mem::RamState,
    ppu::Ppu,
    video::{Video, VideoFilter},
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
    #[error("unimplemented mapper `{0}`")]
    UnimplementedMapper(u16),
    /// Filesystem error.
    #[error(transparent)]
    Fs(#[from] fs::Error),
    /// IO error.
    #[error("{context}: {source:?}")]
    Io {
        context: String,
        source: std::io::Error,
    },
}

impl Error {
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
    pub fn set(&mut self, rev: MapperRevision) {
        match rev {
            MapperRevision::Mmc3(rev) => self.mmc3 = rev,
            MapperRevision::Bf909(rev) => self.bf909 = rev,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[must_use]
/// Control deck configuration settings.
pub struct Config {
    /// Whether to emulate the NES with cycle accuracy or not. Increased CPU use, but more accurate
    /// emulation.
    pub cycle_accurate: bool,
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
}

impl Config {
    /// Base directory for storing TetaNES data.
    pub const BASE_DIR: &'static str = "tetanes";
    /// Directory for storing battery-backed Cart RAM.
    pub const SRAM_DIR: &'static str = "sram";

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
            cycle_accurate: true,
            filter: VideoFilter::default(),
            region: NesRegion::Auto,
            ram_state: RamState::Random,
            four_player: FourPlayer::default(),
            zapper: false,
            genie_codes: vec![],
            concurrent_dpad: false,
            channels_enabled: [true; Apu::MAX_CHANNEL_COUNT],
            headless_mode: HeadlessMode::empty(),
            data_dir: Self::default_data_dir(),
            mapper_revisions: MapperRevisionsConfig::default(),
            emulate_ppu_warmup: false,
        }
    }
}

/// Represents a loaded ROM [`Cart`].
#[derive(Debug, Clone)]
pub struct LoadedRom {
    /// Name of ROM.
    pub name: String,
    /// Whether the loaded Cart is battery-backed.
    pub battery_backed: bool,
    /// Auto-detected of the loaded Cart.
    pub region: NesRegion,
}

/// Represents an NES Control Deck. Encapsulates the entire emulation state.
#[derive(Debug, Clone)]
#[must_use]
pub struct ControlDeck {
    /// Whether a ROM is loaded and the emulation is currently running or not.
    running: bool,
    /// Video output and filtering.
    video: Video,
    /// Last frame number rendered, allowing `frame_buffer` to be cached if called multiple times.
    last_frame_number: u32,
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
    /// Emulated frame speed ranging from 0.25 to 2.0.
    frame_speed: f32,
    /// Accumulated frame speed to account for slower 1x speeds.
    frame_accumulator: f32,
    /// NES CPU.
    cpu: Cpu,
}

impl Default for ControlDeck {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlDeck {
    /// Create a NES `ControlDeck` with the default configuration.
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    /// Create a NES `ControlDeck` with a configuration.
    pub fn with_config(cfg: Config) -> Self {
        let mut cpu = Cpu::new(Bus::new(cfg.region, cfg.ram_state));
        cpu.bus.ppu.skip_rendering = cfg.headless_mode.contains(HeadlessMode::NO_VIDEO);
        cpu.bus.ppu.emulate_warmup = cfg.emulate_ppu_warmup;
        cpu.bus.apu.skip_mixing = cfg.headless_mode.contains(HeadlessMode::NO_AUDIO);
        if cfg.region.is_auto() {
            cpu.set_region(NesRegion::Ntsc);
        } else {
            cpu.set_region(cfg.region);
        }
        cpu.bus.input.set_concurrent_dpad(cfg.concurrent_dpad);
        cpu.bus.input.set_four_player(cfg.four_player);
        cpu.bus.input.connect_zapper(cfg.zapper);
        for (i, enabled) in cfg.channels_enabled.iter().enumerate() {
            cpu.bus
                .apu
                .set_channel_enabled(Channel::try_from(i).expect("valid APU channel"), *enabled);
        }
        for genie_code in cfg.genie_codes.iter().cloned() {
            cpu.bus.add_genie_code(genie_code);
        }
        let video = Video::with_filter(cfg.filter);
        Self {
            running: false,
            video,
            last_frame_number: 0,
            loaded_rom: None,
            sram_dir: cfg.sram_dir(),
            mapper_revisions: cfg.mapper_revisions,
            auto_detect_region: cfg.region.is_auto(),
            cycles_remaining: 0.0,
            frame_speed: 1.0,
            frame_accumulator: 0.0,
            cpu,
        }
    }

    /// Returns the path to the SRAM save file for a given ROM name which is used to store
    /// battery-backed Cart RAM. Returns `None` when the current platform doesn't have a
    /// `data` directory and no custom `data_dir` was configured.
    pub fn sram_dir(&self, name: &str) -> PathBuf {
        self.sram_dir.join(name)
    }

    /// Loads a ROM cartridge into memory
    ///
    /// # Errors
    ///
    /// If there is any issue loading the ROM, then an error is returned.
    pub fn load_rom<S: ToString, F: Read>(&mut self, name: S, rom: &mut F) -> Result<LoadedRom> {
        let name = name.to_string();
        self.unload_rom()?;
        let cart = Cart::from_rom(&name, rom, self.cpu.bus.ram_state)?;
        if cart.mapper.is_none() {
            return Err(Error::UnimplementedMapper(cart.mapper_num()));
        }
        let loaded_rom = LoadedRom {
            name: name.clone(),
            battery_backed: cart.battery_backed(),
            region: cart.region(),
        };
        if self.auto_detect_region {
            self.cpu.set_region(loaded_rom.region);
        }
        self.cpu.bus.load_cart(cart);
        self.update_mapper_revisions();
        self.reset(ResetKind::Hard);
        self.running = true;
        let sram_dir = self.sram_dir(&name);
        if let Err(err) = self.load_sram(sram_dir) {
            error!("failed to load SRAM: {err:?}");
        }
        self.loaded_rom = Some(loaded_rom.clone());
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
            let sram_dir = self.sram_dir(&rom.name);
            if let Err(err) = self.save_sram(sram_dir) {
                error!("failed to save SRAM: {err:?}");
            }
        }
        self.loaded_rom = None;
        self.cpu.bus.unload_cart();
        self.running = false;
        Ok(())
    }

    /// Load a previously saved CPU state.
    #[inline]
    pub fn load_cpu(&mut self, cpu: Cpu) {
        self.cpu.load(cpu);
    }

    /// Set the [`MapperRevision`] to emulate for the any ROM loaded that uses this mapper.
    #[inline]
    pub fn set_mapper_revision(&mut self, rev: MapperRevision) {
        self.mapper_revisions.set(rev);
        self.update_mapper_revisions();
    }

    /// Set the set of [`MapperRevisionsConfig`] to emulate for the any ROM loaded that uses this
    /// mapper.
    #[inline]
    pub fn set_mapper_revisions(&mut self, revs: MapperRevisionsConfig) {
        self.mapper_revisions = revs;
        self.update_mapper_revisions();
    }

    /// Internal method to update the loaded ROM mapper revision when `mapper_revisions` is
    /// updated.
    fn update_mapper_revisions(&mut self) {
        match &mut self.cpu.bus.ppu.bus.mapper {
            Mapper::Txrom(mapper) => {
                mapper.set_revision(self.mapper_revisions.mmc3);
            }
            Mapper::Bf909x(mapper) => {
                mapper.set_revision(self.mapper_revisions.bf909);
            }
            _ => (),
        }
    }

    /// Set whether concurrent D-Pad input is enabled which wasn't possible on the original NES.
    #[inline]
    pub fn set_concurrent_dpad(&mut self, enabled: bool) {
        self.cpu.bus.input.set_concurrent_dpad(enabled);
    }

    /// Set whether emulation should be cycle accurate or not. Disabling this can increase
    /// performance.
    #[inline]
    pub fn set_cycle_accurate(&mut self, enabled: bool) {
        self.cpu.cycle_accurate = enabled;
    }

    /// Set emulation RAM initialization state.
    #[inline]
    pub fn set_ram_state(&mut self, ram_state: RamState) {
        self.cpu.bus.ram_state = ram_state;
    }

    /// Set the headless mode which can increase performance when the frame and audio outputs are
    /// not needed.
    #[inline]
    pub fn set_headless_mode(&mut self, mode: HeadlessMode) {
        self.cpu.bus.ppu.skip_rendering = mode.contains(HeadlessMode::NO_VIDEO);
        self.cpu.bus.apu.skip_mixing = mode.contains(HeadlessMode::NO_AUDIO);
    }

    /// Set whether to emulate PPU warmup where writes to certain registers are ignored. Can result
    /// in some games not working correctly.
    ///
    /// See: <https://www.nesdev.org/wiki/PPU_power_up_state>
    #[inline]
    pub fn set_emulate_ppu_warmup(&mut self, enabled: bool) {
        self.cpu.bus.ppu.emulate_warmup = enabled;
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
    pub fn wram(&self) -> &[u8] {
        self.cpu.bus.wram()
    }

    /// Returns the battery-backed Save RAM.
    #[inline]
    #[must_use]
    pub fn sram(&self) -> &[u8] {
        self.cpu.bus.sram()
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
            self.cpu.bus.save(path).map_err(Error::Sram)?;
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
                self.cpu.bus.load(path).map_err(Error::Sram)?;
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
        fs::save(path, &self.cpu).map_err(Error::SaveState)
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
            fs::load::<Cpu>(path)
                .map_err(Error::SaveState)
                .map(|mut cpu| {
                    cpu.bus.input.clear();
                    self.load_cpu(cpu)
                })
        } else {
            Err(Error::NoSaveStateFound)
        }
    }

    /// Load the raw underlying frame buffer from the PPU for further processing.
    pub fn frame_buffer_raw(&mut self) -> &[u16] {
        self.cpu.bus.ppu.frame_buffer()
    }

    /// Load a frame worth of pixels.
    #[inline]
    pub fn frame_buffer(&mut self) -> &[u8] {
        // Avoid applying filter if the frame number hasn't changed
        let frame_number = self.cpu.bus.ppu.frame_number();
        if self.last_frame_number == frame_number {
            return &self.video.frame;
        }

        self.last_frame_number = frame_number;
        self.video
            .apply_filter(self.cpu.bus.ppu.frame_buffer(), frame_number)
    }

    /// Load a frame worth of pixels into the given buffer.
    #[inline]
    pub fn frame_buffer_into(&self, buffer: &mut [u8]) {
        self.video.apply_filter_into(
            self.cpu.bus.ppu.frame_buffer(),
            self.cpu.bus.ppu.frame_number(),
            buffer,
        );
    }

    /// Get the current frame number.
    #[inline]
    #[must_use]
    pub const fn frame_number(&self) -> u32 {
        self.cpu.bus.ppu.frame_number()
    }

    /// Get audio samples.
    #[inline]
    #[must_use]
    pub fn audio_samples(&self) -> &[f32] {
        self.cpu.bus.audio_samples()
    }

    /// Clear audio samples.
    #[inline]
    pub fn clear_audio_samples(&mut self) {
        self.cpu.bus.clear_audio_samples();
    }

    /// CPU clock rate based on currently configured NES region.
    #[inline]
    #[must_use]
    pub const fn clock_rate(&self) -> f32 {
        self.cpu.clock_rate()
    }

    /// Steps the control deck one CPU clock.
    ///
    /// # Errors
    ///
    /// If CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_instr(&mut self) -> Result<usize> {
        if !self.running {
            return Err(Error::RomNotLoaded);
        }
        let cycles = self.clock();
        if self.cpu_corrupted() {
            self.running = false;
            return Err(Error::CpuCorrupted);
        }
        Ok(cycles)
    }

    /// Steps the control deck the number of seconds.
    ///
    /// # Errors
    ///
    /// If CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_seconds(&mut self, seconds: f32) -> Result<usize> {
        self.cycles_remaining += self.clock_rate() * seconds;
        let mut total_cycles = 0;
        while self.cycles_remaining > 0.0 {
            let cycles = self.clock_instr()?;
            total_cycles += cycles;
            self.cycles_remaining -= cycles as f32;
        }
        Ok(total_cycles)
    }

    /// Steps the control deck an entire frame.
    ///
    /// # Errors
    ///
    /// If CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_frame(&mut self) -> Result<usize> {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        // Frames that aren't multiples of the default render 1 more/less frames
        // every other frame
        // e.g. a speed of 1.5 will clock # of frames: 1, 2, 1, 2, 1, 2, 1, 2, ...
        // A speed of 0.5 will clock 0, 1, 0, 1, 0, 1, 0, 1, 0, ...
        self.frame_accumulator += self.frame_speed;
        let mut frames_to_clock = 0;
        while self.frame_accumulator >= 1.0 {
            self.frame_accumulator -= 1.0;
            frames_to_clock += 1;
        }

        let mut total_cycles = 0;
        for _ in 0..frames_to_clock {
            let frame = self.frame_number();
            while frame == self.frame_number() {
                total_cycles += self.clock_instr()?;
            }
        }
        self.cpu.bus.apu.clock_flush();

        Ok(total_cycles)
    }

    /// Steps the control deck an entire frame, calling `handle_output` with the `cycles`, `frame_buffer` and
    /// `audio_samples` for that frame.
    ///
    /// # Errors
    ///
    /// If CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_frame_output<T>(
        &mut self,
        handle_output: impl FnOnce(usize, &[u8], &[f32]) -> T,
    ) -> Result<T> {
        let cycles = self.clock_frame()?;
        let frame = self.video.apply_filter(
            self.cpu.bus.ppu.frame_buffer(),
            self.cpu.bus.ppu.frame_number(),
        );
        let audio = self.cpu.bus.audio_samples();
        let res = handle_output(cycles, frame, audio);
        self.cpu.bus.clear_audio_samples();
        Ok(res)
    }

    /// Steps the control deck an entire frame, copying the `frame_buffer` and
    /// `audio_samples` for that frame into the provided buffers.
    ///
    /// # Errors
    ///
    /// If CPU encounteres an invalid opcode, an error is returned.
    pub fn clock_frame_into(
        &mut self,
        frame_buffer: &mut [u8],
        audio_samples: &mut [f32],
    ) -> Result<usize> {
        let cycles = self.clock_frame()?;
        let frame = self.video.apply_filter(
            self.cpu.bus.ppu.frame_buffer(),
            self.cpu.bus.ppu.frame_number(),
        );
        frame_buffer.copy_from_slice(&frame[..frame_buffer.len()]);
        let audio = self.cpu.bus.audio_samples();
        audio_samples.copy_from_slice(&audio[..audio_samples.len()]);
        self.clear_audio_samples();
        Ok(cycles)
    }

    /// Steps the control deck an entire frame with run-ahead frames to reduce input lag.
    ///
    /// # Errors
    ///
    /// If CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_frame_ahead<T>(
        &mut self,
        run_ahead: usize,
        handle_output: impl FnOnce(usize, &[u8], &[f32]) -> T,
    ) -> Result<T> {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if run_ahead == 0 {
            return self.clock_frame_output(handle_output);
        }

        // Clock current frame and save state so we can rewind
        self.clock_frame()?;
        let frame = std::mem::take(&mut self.cpu.bus.ppu.frame.buffer);
        // Save state so we can rewind
        let state = bincode::serialize(&self.cpu)
            .map_err(|err| fs::Error::SerializationFailed(err.to_string()))?;

        // Clock additional frames and discard video/audio
        self.cpu.bus.ppu.skip_rendering = true;
        for _ in 1..run_ahead {
            self.clock_frame()?;
        }
        self.cpu.bus.ppu.skip_rendering = false;

        // Output the future frame video/audio
        self.clear_audio_samples();
        let result = self.clock_frame_output(handle_output)?;

        // Restore back to current frame
        let mut state = bincode::deserialize::<Cpu>(&state)
            .map_err(|err| fs::Error::DeserializationFailed(err.to_string()))?;
        state.bus.ppu.frame.buffer = frame;
        self.load_cpu(state);

        Ok(result)
    }

    /// Steps the control deck an entire frame with run-ahead frames to reduce input lag.
    ///
    /// # Errors
    ///
    /// If CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_frame_ahead_into(
        &mut self,
        run_ahead: usize,
        frame_buffer: &mut [u8],
        audio_samples: &mut [f32],
    ) -> Result<usize> {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if run_ahead == 0 {
            return self.clock_frame_into(frame_buffer, audio_samples);
        }

        // Clock current frame and save state so we can rewind
        self.clock_frame()?;
        let frame = std::mem::take(&mut self.cpu.bus.ppu.frame.buffer);
        // Save state so we can rewind
        let state = bincode::serialize(&self.cpu)
            .map_err(|err| fs::Error::SerializationFailed(err.to_string()))?;

        // Clock additional frames and discard video/audio
        for _ in 1..run_ahead {
            self.clock_frame()?;
        }

        // Output the future frame/audio
        self.clear_audio_samples();
        let cycles = self.clock_frame_into(frame_buffer, audio_samples)?;

        // Restore back to current frame
        let mut state = bincode::deserialize::<Cpu>(&state)
            .map_err(|err| fs::Error::DeserializationFailed(err.to_string()))?;
        state.bus.ppu.frame.buffer = frame;
        self.load_cpu(state);

        Ok(cycles)
    }

    /// Steps the control deck a single scanline.
    ///
    /// # Errors
    ///
    /// If CPU encounters an invalid opcode, then an error is returned.
    pub fn clock_scanline(&mut self) -> Result<usize> {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut total_cycles = 0;
        let current_scanline = self.cpu.bus.ppu.scanline;
        while current_scanline == self.cpu.bus.ppu.scanline {
            total_cycles += self.clock_instr()?;
        }
        Ok(total_cycles)
    }

    /// Returns whether the CPU is corrupted or not which means it encounted an invalid/unhandled
    /// opcode and can't proceed executing the current ROM.
    #[inline]
    #[must_use]
    pub const fn cpu_corrupted(&self) -> bool {
        self.cpu.corrupted
    }

    /// Returns the current [`Cpu`] state.
    #[inline]
    pub const fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// Returns a mutable reference to the current [`Cpu`] state.
    #[inline]
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    /// Returns the current [`Ppu`] state.
    #[inline]
    pub const fn ppu(&self) -> &Ppu {
        &self.cpu.bus.ppu
    }

    /// Returns a mutable reference to the current [`Ppu`] state.
    #[inline]
    pub fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.cpu.bus.ppu
    }

    /// Retu[ns the current [`Bus`] state.
    #[inline]
    pub const fn bus(&self) -> &Bus {
        &self.cpu.bus
    }

    /// Returns a mutable reference to the current [`Bus`] state.
    #[inline]
    pub fn bus_mut(&mut self) -> &mut Bus {
        &mut self.cpu.bus
    }

    /// Returns the current [`Apu`] state.
    #[inline]
    pub const fn apu(&self) -> &Apu {
        &self.cpu.bus.apu
    }

    /// Returns a mutable reference to the current [`Apu`] state.
    #[inline]
    pub fn apu_mut(&mut self) -> &Apu {
        &mut self.cpu.bus.apu
    }

    /// Returns the current [`Mapper`] state.
    #[inline]
    pub const fn mapper(&self) -> &Mapper {
        &self.cpu.bus.ppu.bus.mapper
    }

    /// Returns a mutable reference to the current [`Mapper`] state.
    #[inline]
    pub fn mapper_mut(&mut self) -> &mut Mapper {
        &mut self.cpu.bus.ppu.bus.mapper
    }

    /// Returns the current four player mode.
    #[inline]
    pub const fn four_player(&self) -> FourPlayer {
        self.cpu.bus.input.four_player
    }

    /// Enable/Disable Four Score for 4-player controllers.
    #[inline]
    pub fn set_four_player(&mut self, four_player: FourPlayer) {
        self.cpu.bus.input.set_four_player(four_player);
    }

    /// Returns the current [`Joypad`] state for a given controller slot.
    #[inline]
    pub fn joypad(&mut self, slot: Player) -> &Joypad {
        self.cpu.bus.input.joypad(slot)
    }

    /// Returns a mutable reference to the current [`Joypad`] state for a given controller slot.
    #[inline]
    pub fn joypad_mut(&mut self, slot: Player) -> &mut Joypad {
        self.cpu.bus.input.joypad_mut(slot)
    }

    /// Returns whether the [`Zapper`](crate::input::Zapper) gun is connected.
    #[inline]
    pub const fn zapper_connected(&self) -> bool {
        self.cpu.bus.input.zapper.connected
    }

    /// Enable [`Zapper`](crate::input::Zapper) gun.
    #[inline]
    pub fn connect_zapper(&mut self, enabled: bool) {
        self.cpu.bus.input.connect_zapper(enabled);
    }

    /// Returns the current [`Zapper`](crate::input::Zapper) aim position.
    #[inline]
    #[must_use]
    pub const fn zapper_pos(&self) -> (u32, u32) {
        let zapper = self.cpu.bus.input.zapper;
        (zapper.x(), zapper.y())
    }

    /// Trigger [`Zapper`](crate::input::Zapper) gun.
    #[inline]
    pub fn trigger_zapper(&mut self) {
        self.cpu.bus.input.zapper.trigger();
    }

    /// Aim [`Zapper`](crate::input::Zapper) gun.
    #[inline]
    pub fn aim_zapper(&mut self, x: u32, y: u32) {
        self.cpu.bus.input.zapper.aim(x, y);
    }

    /// Set the video filter for frame buffer output when calling [`ControlDeck::frame_buffer`].
    #[inline]
    pub fn set_filter(&mut self, filter: VideoFilter) {
        self.video.filter = filter;
    }

    /// Set the [`Apu`] sample rate.
    #[inline]
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.cpu.bus.apu.set_sample_rate(sample_rate);
    }

    /// Set the emulation speed.
    #[inline]
    pub fn set_frame_speed(&mut self, speed: f32) {
        self.frame_speed = speed;
        self.cpu.bus.apu.set_frame_speed(speed);
    }

    /// Add a NES Game Genie code.
    ///
    /// # Errors
    ///
    /// If the genie code is invalid, an error is returned.
    #[inline]
    pub fn add_genie_code(&mut self, genie_code: String) -> Result<()> {
        self.cpu.bus.add_genie_code(GenieCode::new(genie_code)?);
        Ok(())
    }

    /// Remove a NES Game Genie code.
    #[inline]
    pub fn remove_genie_code(&mut self, genie_code: &str) {
        self.cpu.bus.remove_genie_code(genie_code);
    }

    /// Remove all NES Game Genie codes.
    #[inline]
    pub fn clear_genie_codes(&mut self) {
        self.cpu.bus.clear_genie_codes();
    }

    /// Returns whether a given [`Apu`] [`Channel`] is enabled.
    #[inline]
    #[must_use]
    pub const fn channel_enabled(&self, channel: Channel) -> bool {
        self.cpu.bus.apu.channel_enabled(channel)
    }

    /// Enable or disable a given [`Apu`] [`Channel`].
    #[inline]
    pub fn set_apu_channel_enabled(&mut self, channel: Channel, enabled: bool) {
        self.cpu.bus.apu.set_channel_enabled(channel, enabled);
    }

    /// Toggle a given [`Apu`] [`Channel`].
    #[inline]
    pub fn toggle_apu_channel(&mut self, channel: Channel) {
        self.cpu.bus.apu.toggle_channel(channel);
    }

    /// Returns whether the control deck is currently running.
    #[inline]
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }
}

impl Clock for ControlDeck {
    /// Steps the control deck a single clock cycle.
    fn clock(&mut self) -> usize {
        self.cpu.clock()
    }
}

impl Regional for ControlDeck {
    /// Get the NES format for the emulation.
    fn region(&self) -> NesRegion {
        self.cpu.region
    }

    /// Set the NES format for the emulation.
    fn set_region(&mut self, region: NesRegion) {
        self.auto_detect_region = region.is_auto();
        if self.auto_detect_region {
            self.cpu.set_region(self.cart_region().unwrap_or_default());
        } else {
            self.cpu.set_region(region);
        }
    }
}

impl Reset for ControlDeck {
    /// Resets the console.
    fn reset(&mut self, kind: ResetKind) {
        self.cpu.reset(kind);
        if self.loaded_rom.is_some() {
            self.running = true;
        }
    }
}
