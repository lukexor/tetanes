use crate::{
    apu::{Apu, Channel},
    bus::Bus,
    cart::{self, Cart},
    common::{Clock, NesRegion, Regional, Reset, ResetKind},
    cpu::Cpu,
    fs,
    genie::{self, GenieCode},
    input::{FourPlayer, Joypad, Player},
    mapper::Mapper,
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

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[must_use]
pub enum Error {
    #[error(transparent)]
    Cart(#[from] cart::Error),
    #[error("sram error: {0:?}")]
    Sram(fs::Error),
    #[error("save state error: {0:?}")]
    SaveState(fs::Error),
    #[error("no rom is loaded")]
    RomNotLoaded,
    #[error("cpu state is corrupted")]
    CpuCorrupted,
    #[error(transparent)]
    InvalidGenieCode(#[from] genie::Error),
    #[error("invalid rom path {0:?}")]
    InvalidRomPath(PathBuf),
    #[error("invalid file path {0:?}")]
    InvalidFilePath(PathBuf),
    #[error(transparent)]
    Fs(#[from] fs::Error),
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
    /// Headless mode flags to disable audio and video rendering.
    #[derive(Default, Debug, Copy, Clone, PartialEq, Serialize, Deserialize, )]
    #[must_use]
    pub struct HeadlessMode: u8 {
        const NO_AUDIO = 0x01;
        const NO_VIDEO = 0x02;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
/// Control deck configuration settings.
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
}

impl Config {
    pub const BASE_DIR: &'static str = "tetanes";
    pub const SRAM_DIR: &'static str = "sram";

    /// Returns the base directory where TetaNES data is stored.
    #[inline]
    #[must_use]
    pub fn data_dir() -> Option<PathBuf> {
        dirs::data_local_dir().map(|dir| dir.join(Self::BASE_DIR))
    }

    /// Returns the path to the SRAM save file for a given ROM name which is used to store
    /// battery-backed Cart RAM.
    #[inline]
    #[must_use]
    pub fn sram_path(name: &str) -> Option<PathBuf> {
        Self::data_dir().map(|dir| dir.join(Self::SRAM_DIR).join(name).with_extension("sram"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            filter: VideoFilter::default(),
            region: NesRegion::default(),
            ram_state: RamState::Random,
            four_player: FourPlayer::default(),
            zapper: false,
            genie_codes: vec![],
            concurrent_dpad: false,
            channels_enabled: [true; Apu::MAX_CHANNEL_COUNT],
            headless_mode: HeadlessMode::empty(),
        }
    }
}

/// Represents an NES Control Deck
#[derive(Debug, Clone)]
#[must_use]
pub struct ControlDeck {
    pub running: bool,
    pub video: Video,
    pub last_frame_number: u32,
    pub loaded_rom: Option<String>,
    pub cart_battery_backed: bool,
    pub cart_region: NesRegion,
    pub region_auto_detect: bool,
    pub cycles_remaining: f32,
    pub cpu: Cpu,
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
    pub fn with_config(config: Config) -> Self {
        let mut cpu = Cpu::new(Bus::new(config.ram_state));
        cpu.bus.ppu.skip_rendering = config.headless_mode.contains(HeadlessMode::NO_VIDEO);
        cpu.bus.apu.skip_mixing = config.headless_mode.contains(HeadlessMode::NO_AUDIO);
        cpu.set_region(config.region);
        cpu.bus.input.set_four_player(config.four_player);
        cpu.bus.input.connect_zapper(config.zapper);
        for (i, enabled) in config.channels_enabled.iter().enumerate() {
            cpu.bus
                .apu
                .set_channel_enabled(Channel::try_from(i).expect("valid APU channel"), *enabled);
        }
        for genie_code in config.genie_codes.iter().cloned() {
            cpu.bus.add_genie_code(genie_code);
        }
        let video = Video::with_filter(config.filter);
        Self {
            running: false,
            video,
            last_frame_number: 0,
            loaded_rom: None,
            cart_battery_backed: false,
            cart_region: NesRegion::default(),
            region_auto_detect: config.region.is_auto(),
            cycles_remaining: 0.0,
            cpu,
        }
    }

    /// Loads a ROM cartridge into memory
    ///
    /// # Errors
    ///
    /// If there is any issue loading the ROM, then an error is returned.
    pub fn load_rom<S: ToString, F: Read>(&mut self, name: S, rom: &mut F) -> Result<()> {
        let name = name.to_string();
        self.unload_rom()?;
        let cart = Cart::from_rom(&name, rom, self.cpu.bus.ram_state)?;
        self.cart_battery_backed = cart.battery_backed();
        self.cart_region = cart.region();
        if self.region_auto_detect {
            self.cpu.set_region(self.cart_region);
        }
        self.cpu.bus.load_cart(cart);
        self.reset(ResetKind::Hard);
        self.running = true;
        if let Some(path) = Config::sram_path(&name) {
            if let Err(err) = self.load_sram(path) {
                error!("failed to load SRAM: {err:?}");
            }
        }
        self.loaded_rom = Some(name);
        Ok(())
    }

    /// Loads a ROM cartridge into memory from a path.
    pub fn load_rom_path(&mut self, path: impl AsRef<std::path::Path>) -> Result<()> {
        use std::{fs::File, io::BufReader};

        let path = path.as_ref();
        let filename = fs::filename(path);
        info!("loading ROM: {filename}");
        File::open(path)
            .map_err(|err| Error::io(err, format!("failed to open rom {path:?}")))
            .and_then(|rom| self.load_rom(filename, &mut BufReader::new(rom)))
    }

    /// Unloads the currently loaded ROM and saves SRAM to disk if the Cart is battery-backed.
    pub fn unload_rom(&mut self) -> Result<()> {
        if let Some(ref rom) = self.loaded_rom {
            if let Some(dir) = Config::sram_path(rom) {
                if let Err(err) = self.save_sram(dir) {
                    error!("failed to save SRAM: {err:?}");
                }
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

    /// Set whether emulation should be cycle accurate or not. Disabling this can increase
    /// performance.
    #[inline]
    pub fn set_cycle_accurate(&mut self, enabled: bool) {
        self.cpu.cycle_accurate = enabled;
    }

    /// Set the headless mode which can increase performance when the frame and audio outputs are
    /// not needed.
    #[inline]
    pub fn set_headless_mode(&mut self, mode: HeadlessMode) {
        self.cpu.bus.ppu.skip_rendering = mode.contains(HeadlessMode::NO_VIDEO);
        self.cpu.bus.apu.skip_mixing = mode.contains(HeadlessMode::NO_AUDIO);
    }

    /// Returns the name of the currently loaded ROM.
    #[inline]
    #[must_use]
    pub const fn loaded_rom(&self) -> &Option<String> {
        &self.loaded_rom
    }

    /// Returns whether the loaded Cart is battery-backed.
    #[inline]
    #[must_use]
    pub fn cart_battery_backed(&self) -> Option<bool> {
        self.loaded_rom.as_ref().map(|_| self.cart_battery_backed)
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
    pub fn save_sram(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if path.is_dir() {
            return Err(Error::InvalidFilePath(path.to_path_buf()));
        }
        if let Some(true) = self.cart_battery_backed() {
            info!("saving SRAM...");
            fs::save(path, self.cpu.bus.sram()).map_err(Error::Sram)?;
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    pub fn load_sram(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if path.is_dir() {
            return Err(Error::InvalidFilePath(path.to_path_buf()));
        }
        if path.is_file() {
            info!("loading SRAM...");
            fs::load(path)
                .map(|data| self.cpu.bus.load_sram(data))
                .map_err(Error::Sram)?;
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
        if path.exists() {
            fs::load::<Cpu>(path)
                .map_err(Error::SaveState)
                .map(|mut cpu| {
                    cpu.bus.input.clear();
                    self.load_cpu(cpu)
                })
        } else {
            Ok(())
        }
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
    /// If CPU encounteres an invalid opcode, an error is returned.
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
    /// If CPU encounteres an invalid opcode, an error is returned.
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
    /// If CPU encounteres an invalid opcode, an error is returned.
    pub fn clock_frame(&mut self) -> Result<usize> {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut total_cycles = 0;
        let frame = self.frame_number();
        while frame == self.frame_number() {
            total_cycles += self.clock_instr()?;
        }
        self.cpu.bus.apu.clock_flush();

        Ok(total_cycles)
    }

    /// Steps the control deck an entire frame, calling `handle_output` with the `cycles`, `frame_buffer` and
    /// `audio_samples` for that frame.
    ///
    /// # Errors
    ///
    /// If CPU encounteres an invalid opcode, an error is returned.
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
    /// If CPU encounteres an invalid opcode, an error is returned.
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

        // Clock current frame and discard video
        self.clock_frame()?;
        // Save state so we can rewind
        let state = bincode::serialize(&self.cpu)
            .map_err(|err| fs::Error::SerializationFailed(err.to_string()))?;

        // Clock additional frames and discard video/audio
        for _ in 1..run_ahead {
            self.clock_frame()?;
        }

        // Output the future frame video/audio
        self.clear_audio_samples();
        let result = self.clock_frame_output(handle_output)?;

        // Restore back to current frame
        let state = bincode::deserialize(&state)
            .map_err(|err| fs::Error::DeserializationFailed(err.to_string()))?;
        self.load_cpu(state);

        Ok(result)
    }

    /// Steps the control deck an entire frame with run-ahead frames to reduce input lag.
    ///
    /// # Errors
    ///
    /// If CPU encounteres an invalid opcode, an error is returned.
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

        // Clock current frame and discard video
        self.clock_frame()?;
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
        let state = bincode::deserialize(&state)
            .map_err(|err| fs::Error::DeserializationFailed(err.to_string()))?;
        self.load_cpu(state);

        Ok(cycles)
    }

    /// Steps the control deck a single scanline.
    ///
    /// # Errors
    ///
    /// If CPU encounteres an invalid opcode, an error is returned.
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

    /// Returns the current CPU state.
    #[inline]
    pub const fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// Returns a mutable reference to the current CPU state.
    #[inline]
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    /// Returns the current PPU state.
    #[inline]
    pub const fn ppu(&self) -> &Ppu {
        &self.cpu.bus.ppu
    }

    /// Returns a mutable reference to the current PPU state.
    #[inline]
    pub fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.cpu.bus.ppu
    }

    /// Returns the current APU state.
    #[inline]
    pub const fn apu(&self) -> &Apu {
        &self.cpu.bus.apu
    }

    /// Returns a mutable reference to the current APU state.
    #[inline]
    pub fn apu_mut(&mut self) -> &Apu {
        &mut self.cpu.bus.apu
    }

    /// Returns the current Mapper state.
    #[inline]
    pub const fn mapper(&self) -> &Mapper {
        &self.cpu.bus.ppu.bus.mapper
    }

    /// Returns a mutable reference to the current Mapper state.
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

    /// Returns the current joypad state for a given controller slot.
    #[inline]
    pub fn joypad(&mut self, slot: Player) -> &Joypad {
        self.cpu.bus.input.joypad(slot)
    }

    /// Returns a mutable reference to the current joypad state for a given controller slot.
    #[inline]
    pub fn joypad_mut(&mut self, slot: Player) -> &mut Joypad {
        self.cpu.bus.input.joypad_mut(slot)
    }

    /// Enable Zapper gun.
    #[inline]
    pub fn connect_zapper(&mut self, enabled: bool) {
        self.cpu.bus.input.connect_zapper(enabled);
    }

    /// Returns the current Zapper gun position.
    #[inline]
    #[must_use]
    pub const fn zapper_pos(&self) -> (u32, u32) {
        let zapper = self.cpu.bus.input.zapper;
        (zapper.x(), zapper.y())
    }

    /// Trigger Zapper gun.
    #[inline]
    pub fn trigger_zapper(&mut self) {
        self.cpu.bus.input.zapper.trigger();
    }

    /// Aim Zapper gun.
    #[inline]
    pub fn aim_zapper(&mut self, x: u32, y: u32) {
        self.cpu.bus.input.zapper.aim(x, y);
    }

    /// Set the video filter for frame buffer output when calling [`ControlDeck::frame_buffer`].
    #[inline]
    pub fn set_filter(&mut self, filter: VideoFilter) {
        self.video.filter = filter;
    }

    /// Set the emulation speed.
    #[inline]
    pub fn set_frame_speed(&mut self, speed: f32) {
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

    /// Returns whether a given APU audio channel is enabled.
    #[inline]
    #[must_use]
    pub const fn channel_enabled(&self, channel: Channel) -> bool {
        self.cpu.bus.apu.channel_enabled(channel)
    }

    /// Toggle a given APU audio channel.
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
        self.region_auto_detect = region.is_auto();
        self.cpu.set_region(region);
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
