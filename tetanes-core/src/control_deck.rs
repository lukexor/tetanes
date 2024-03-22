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
use serde::{Deserialize, Serialize};
use std::{io::Read, path::PathBuf};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
/// Control deck configuration settings.
pub struct Config {
    /// Directory where config is stored.
    pub dir: PathBuf,
    /// Video filter.
    pub filter: VideoFilter,
    /// Audio device sample rate.
    pub sample_rate: f32,
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
    /// Save state slot.
    pub save_slot: u8,
    /// Load save state on loading a ROM.
    pub load_on_start: bool,
    /// Save state on unloading a ROM.
    pub save_on_exit: bool,
    /// Whether to support concurrent D-Pad input which wasn't possible on the original NES.
    pub concurrent_dpad: bool,
    /// Apu channels enabled.
    pub channels_enabled: [bool; 5],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dir: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("./"))
                .join(Self::DIR),
            filter: VideoFilter::default(),
            sample_rate: Apu::DEFAULT_SAMPLE_RATE,
            region: NesRegion::default(),
            ram_state: RamState::Random,
            four_player: FourPlayer::default(),
            zapper: false,
            genie_codes: vec![],
            load_on_start: true,
            save_on_exit: true,
            save_slot: 1,
            concurrent_dpad: false,
            channels_enabled: [true; 5],
        }
    }
}

impl Config {
    pub const DIR: &'static str = ".config/tetanes";
    pub const SAVE_DIR: &'static str = "save";
    pub const SRAM_DIR: &'static str = "sram";

    #[must_use]
    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    #[must_use]
    pub fn save_dir(&self) -> PathBuf {
        self.dir.join(Self::SAVE_DIR)
    }

    #[must_use]
    pub fn sram_dir(&self) -> PathBuf {
        self.dir.join(Self::SRAM_DIR)
    }
}

/// Represents an NES Control Deck
#[derive(Debug, Clone)]
#[must_use]
pub struct ControlDeck {
    config: Config,
    running: bool,
    video: Video,
    loaded_rom: Option<String>,
    cart_battery_backed: bool,
    cycles_remaining: f32,
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
    pub fn with_config(config: Config) -> Self {
        let mut cpu = Cpu::new(Bus::new(config.ram_state, config.sample_rate));
        cpu.set_region(config.region);
        cpu.bus.input.set_four_player(config.four_player);
        cpu.bus.input.connect_zapper(config.zapper);
        for genie_code in config.genie_codes.iter().cloned() {
            cpu.bus.add_genie_code(genie_code);
        }
        let video = Video::with_filter(config.filter);
        Self {
            config,
            running: false,
            video,
            loaded_rom: None,
            cart_battery_backed: false,
            cycles_remaining: 0.0,
            cpu,
        }
    }

    /// Loads a ROM cartridge into memory
    ///
    /// # Errors
    ///
    /// If there is any issue loading the ROM, then an error is returned.
    pub fn load_rom<S: ToString, F: Read>(&mut self, name: S, rom: &mut F) -> Result<NesRegion> {
        self.unload_rom()?;
        self.loaded_rom = Some(name.to_string());
        let cart = Cart::from_rom(name, rom, self.cpu.bus.ram_state)?;
        let region = cart.region();
        self.cart_battery_backed = cart.battery_backed();
        self.set_region(cart.region());
        self.cpu.bus.load_cart(cart);
        self.reset(ResetKind::Hard);
        self.running = true;
        // TODO: Bubble this up to caller while still providing region
        if let Err(err) = self.load_sram() {
            error!("failed to load SRAM: {err:?}");
        }
        if self.config.load_on_start {
            if let Err(err) = self.load_state() {
                error!("failed to load state: {err:?}");
            }
        }
        Ok(region)
    }

    /// Loads a ROM cartridge into memory from a path.
    pub fn load_rom_path(&mut self, path: impl AsRef<std::path::Path>) -> Result<NesRegion> {
        use std::{fs::File, io::BufReader};

        let path = path.as_ref();
        let filename = fs::filename(path);
        info!("loading ROM: {filename}");
        File::open(path)
            .map_err(|err| Error::io(err, format!("failed to open rom {path:?}")))
            .and_then(|rom| self.load_rom(filename, &mut BufReader::new(rom)))
    }

    pub fn unload_rom(&mut self) -> Result<()> {
        if self.loaded_rom.is_some() {
            self.save_sram()?;
            if self.config.save_on_exit {
                self.save_state()?;
            }
        }
        self.loaded_rom = None;
        self.cpu.bus.unload_cart();
        self.running = false;
        Ok(())
    }

    pub fn load_cpu(&mut self, cpu: Cpu) {
        self.cpu = cpu;
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    #[must_use]
    pub const fn loaded_rom(&self) -> &Option<String> {
        &self.loaded_rom
    }

    /// Returns whether the loaded Cart is battery-backed.
    #[must_use]
    pub fn cart_battery_backed(&self) -> Option<bool> {
        self.loaded_rom.as_ref().map(|_| self.cart_battery_backed)
    }

    /// Returns the NES Work RAM.
    #[must_use]
    pub fn wram(&self) -> &[u8] {
        self.cpu.bus.wram()
    }

    /// Returns the battery-backed Save RAM.
    #[must_use]
    pub fn sram(&self) -> &[u8] {
        self.cpu.bus.sram()
    }

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    pub fn save_sram(&self) -> Result<()> {
        if let Some(true) = self.cart_battery_backed() {
            if let Some(sram_path) = self.sram_path() {
                info!("saving SRAM...");
                fs::save(sram_path, self.cpu.bus.sram()).map_err(Error::Sram)?;
            }
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    pub fn load_sram(&mut self) -> Result<()> {
        if let Some(sram_path) = self.sram_path() {
            if sram_path.exists() {
                info!("loading SRAM...");
                fs::load(&sram_path)
                    .map(|data| self.cpu.bus.load_sram(data))
                    .map_err(Error::Sram)?;
            }
        }
        Ok(())
    }

    /// Set the directory where the configuration and save files are stored.
    pub fn set_config_dir(&mut self, dir: impl Into<PathBuf>) {
        self.config.dir = dir.into();
    }

    /// Set the save slot for save states.
    pub fn set_save_slot(&mut self, slot: u8) {
        self.config.save_slot = slot;
    }

    /// Save the current state of the console into a save file.
    ///
    /// # Errors
    ///
    /// If there is an issue saving the state, then an error is returned.
    pub fn save_state(&mut self) -> Result<()> {
        let Some(save_path) = self.save_path() else {
            return Err(Error::RomNotLoaded);
        };
        // Avoid saving any test roms
        if save_path.to_string_lossy().contains("test") {
            return Ok(());
        }
        fs::save(save_path, &self.cpu).map_err(Error::SaveState)
    }

    /// Load the console with data saved from a save state, if it exists.
    ///
    /// # Errors
    ///
    /// If there is an issue loading the save state, then an error is returned.
    pub fn load_state(&mut self) -> Result<()> {
        self.save_path()
            .filter(|path| path.exists())
            .map_or(Ok(()), |save_path| {
                fs::load(save_path)
                    .map_err(Error::SaveState)
                    .map(|cpu| self.load_cpu(cpu))
            })
    }

    /// Load a frame worth of pixels.
    pub fn frame_buffer(&mut self) -> &[u8] {
        self.video.apply_filter(
            self.cpu.bus.ppu.frame_buffer(),
            self.cpu.bus.ppu.frame_number(),
        )
    }

    /// Get the current frame number.
    #[must_use]
    pub const fn frame_number(&self) -> u32 {
        self.cpu.bus.ppu.frame_number()
    }

    /// Get audio samples.
    #[must_use]
    pub fn audio_samples(&self) -> &[f32] {
        self.cpu.bus.audio_samples()
    }

    /// Clear audio samples.
    pub fn clear_audio_samples(&mut self) {
        self.cpu.bus.clear_audio_samples();
    }

    /// CPU clock rate based on currently configured NES region.
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
        Ok(total_cycles)
    }

    /// Steps the control deck a single scanline.
    ///
    /// # Errors
    ///
    /// If CPU encounteres an invalid opcode, an error is returned.
    pub fn clock_scanline(&mut self) -> Result<usize> {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let current_scanline = self.cpu.bus.ppu.scanline();
        let mut total_cycles = 0;
        while current_scanline == self.cpu.bus.ppu.scanline() {
            total_cycles += self.clock_instr()?;
        }
        Ok(total_cycles)
    }

    /// Returns whether the CPU is corrupted or not which means it encounted an invalid/unhandled
    /// opcode and can't proceed executing the current ROM.
    #[must_use]
    pub const fn cpu_corrupted(&self) -> bool {
        self.cpu.corrupted
    }

    pub const fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    pub const fn ppu(&self) -> &Ppu {
        &self.cpu.bus.ppu
    }

    pub fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.cpu.bus.ppu
    }

    pub const fn apu(&self) -> &Apu {
        &self.cpu.bus.apu
    }

    pub const fn mapper(&self) -> &Mapper {
        &self.cpu.bus.ppu.bus.mapper
    }

    pub fn mapper_mut(&mut self) -> &mut Mapper {
        &mut self.cpu.bus.ppu.bus.mapper
    }

    /// Returns whether Four Score is enabled.
    pub const fn four_player(&self) -> FourPlayer {
        self.cpu.bus.input.four_player
    }

    /// Enable/Disable Four Score for 4-player controllers.
    pub fn set_four_player(&mut self, four_player: FourPlayer) {
        self.cpu.bus.input.set_four_player(four_player);
    }

    /// Returns a mutable reference to a joypad.
    pub fn joypad_mut(&mut self, slot: Player) -> &mut Joypad {
        self.cpu.bus.input.joypad_mut(slot)
    }

    /// Returns the zapper aiming position for the given controller slot.
    #[must_use]
    pub const fn zapper_pos(&self) -> (u32, u32) {
        let zapper = self.cpu.bus.input.zapper;
        (zapper.x(), zapper.y())
    }

    /// Trigger Zapper gun for a given controller slot.
    pub fn trigger_zapper(&mut self) {
        self.cpu.bus.input.zapper.trigger();
    }

    /// Aim Zapper gun for a given controller slot.
    pub fn aim_zapper(&mut self, x: u32, y: u32) {
        self.cpu.bus.input.zapper.aim(x, y);
    }

    /// Set the image filter for video output.
    pub fn set_filter(&mut self, filter: VideoFilter) {
        self.video.filter = filter;
    }

    /// Set the APU sample rate (useful for emulation speed changes).
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.cpu.bus.apu.set_sample_rate(sample_rate);
    }

    /// Enable Zapper gun.
    pub fn connect_zapper(&mut self, enabled: bool) {
        self.cpu.bus.input.connect_zapper(enabled);
    }

    /// Add NES Game Genie codes.
    ///
    /// # Errors
    ///
    /// If genie code is invalid, an error is returned.
    pub fn add_genie_code(&mut self, genie_code: String) -> Result<()> {
        self.cpu.bus.add_genie_code(GenieCode::new(genie_code)?);
        Ok(())
    }

    pub fn remove_genie_code(&mut self, genie_code: &str) {
        self.cpu.bus.remove_genie_code(genie_code);
    }

    /// Returns whether a given API audio channel is enabled.
    #[must_use]
    pub const fn channel_enabled(&self, channel: Channel) -> bool {
        self.cpu.bus.apu.channel_enabled(channel)
    }

    /// Toggle one of the APU audio channels.
    pub fn toggle_apu_channel(&mut self, channel: Channel) {
        self.cpu.bus.apu.toggle_channel(channel);
    }

    /// Is control deck running.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// Returns the path where battery-backed Save RAM files are stored if a ROM is loaded. Returns
    /// `None` if no ROM is loaded.
    pub fn sram_path(&self) -> Option<PathBuf> {
        self.loaded_rom()
            .as_ref()
            .map(|rom| self.config.sram_dir().join(rom).with_extension("sram"))
    }

    /// Returns the path where Save states are stored if a ROM is loaded. Returns `None` if no ROM
    /// is loaded.
    pub fn save_path(&self) -> Option<PathBuf> {
        self.loaded_rom().as_ref().map(|rom| {
            self.config
                .save_dir()
                .join(rom)
                .join(self.config.save_slot.to_string())
                .with_extension("save")
        })
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
        self.cpu.set_region(region);
    }
}

impl Reset for ControlDeck {
    /// Resets the console.
    fn reset(&mut self, kind: ResetKind) {
        self.cpu.reset(kind);
    }
}
