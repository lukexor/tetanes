use crate::{
    common::NesRegion,
    control_deck,
    input::Player,
    nes::{
        event::{Action, DeckEvent, Input, InputBinding, InputMap},
        Nes,
    },
    ppu::Ppu,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

const MIN_SPEED: f32 = 0.25; // 25% - 15 Hz
const MAX_SPEED: f32 = 2.0; // 200% - 120 Hz
const WINDOW_WIDTH_NTSC: f32 = Ppu::WIDTH as f32 * 8.0 / 7.0 + 0.5; // for 8:7 Aspect Ratio
const WINDOW_WIDTH_PAL: f32 = Ppu::WIDTH as f32 * 18.0 / 13.0 + 0.5; // for 18:13 Aspect Ratio
const WINDOW_HEIGHT_NTSC: f32 = Ppu::HEIGHT as f32;
const WINDOW_HEIGHT_PAL: f32 = Ppu::HEIGHT as f32;
pub const FRAME_TRIM_PITCH: usize = (4 * Ppu::WIDTH * 8) as usize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
#[serde(default)] // Ensures new fields don't break existing configurations
/// NES emulation configuration settings.
pub struct Config {
    pub control_deck: control_deck::Config,
    pub rom_path: PathBuf,
    pub replay_path: Option<PathBuf>,
    pub show_hidden_files: bool,
    pub audio_enabled: bool,
    pub debug: bool,
    pub fullscreen: bool,
    pub vsync: bool,
    pub threaded: bool,
    pub concurrent_dpad: bool,
    pub frame_rate: f64,
    #[serde(skip)]
    pub target_frame_duration: Duration,
    pub scale: f32,
    pub frame_speed: f32,
    pub rewind: bool,
    pub rewind_interval: u8,
    pub rewind_buffer_size_mb: usize,
    pub controller_deadzone: f64,
    pub audio_sample_rate: f32,
    pub audio_latency: Duration,
    pub input_bindings: Vec<InputBinding>,
    #[serde(skip)]
    pub input_map: InputMap,
}

impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        // To avoid comparing an unsorted input_bindings list
        self.control_deck == other.control_deck
            && self.show_hidden_files == other.show_hidden_files
            && self.audio_enabled == other.audio_enabled
            && self.debug == other.debug
            && self.fullscreen == other.fullscreen
            && self.vsync == other.vsync
            && self.threaded == other.threaded
            && self.concurrent_dpad == other.concurrent_dpad
            && self.frame_rate == other.frame_rate
            && self.target_frame_duration == other.target_frame_duration
            && self.scale == other.scale
            && self.frame_speed == other.frame_speed
            && self.rewind == other.rewind
            && self.rewind_interval == other.rewind_interval
            && self.rewind_buffer_size_mb == other.rewind_buffer_size_mb
            && self.controller_deadzone == other.controller_deadzone
            && self.audio_sample_rate == other.audio_sample_rate
            && self.audio_latency == other.audio_latency
            && self.input_map == other.input_map
    }
}

impl Default for Config {
    fn default() -> Self {
        let frame_rate = 60.0;
        let input_map = InputMap::default();
        Self {
            control_deck: control_deck::Config::default(),
            rom_path: PathBuf::from("./"),
            replay_path: None,
            show_hidden_files: false,
            audio_enabled: true,
            debug: false,
            fullscreen: false,
            vsync: true,
            concurrent_dpad: false,
            threaded: true,
            frame_rate,
            target_frame_duration: Duration::from_secs_f64(frame_rate.recip()),
            scale: 3.0,
            frame_speed: 1.0,
            rewind: true,
            rewind_interval: 2,
            rewind_buffer_size_mb: 20 * 1024 * 1024,
            controller_deadzone: 0.5,
            audio_sample_rate: 44_100.0,
            audio_latency: Duration::from_millis(if cfg!(target_arch = "wasm32") {
                120
            } else {
                30
            }),
            input_bindings: input_map
                .iter()
                .map(|(input, (slot, action))| (*input, *slot, *action))
                .collect(),
            input_map,
        }
    }
}

impl From<Config> for control_deck::Config {
    fn from(config: Config) -> Self {
        config.control_deck
    }
}

impl Config {
    pub const WINDOW_TITLE: &'static str = "TetaNES";
    pub const DIRECTORY: &'static str = control_deck::Config::DIRECTORY;
    pub const FILENAME: &'static str = "config.json";

    #[cfg(target_arch = "wasm32")]
    pub fn load() -> Self {
        log::info!("Loading default configuration");
        // TODO: Load from local storage?
        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load() -> Self {
        use anyhow::Context;
        use std::fs::File;

        let path = Self::path(Self::FILENAME);
        let mut config = if path.exists() {
            log::info!("Loading saved configuration");
            File::open(&path)
                .with_context(|| format!("failed to open {path:?}"))
                .and_then(|file| Ok(serde_json::from_reader::<_, Config>(file)?))
                .with_context(|| format!("failed to parse {path:?}"))
                .unwrap_or_else(|err| {
                    log::error!("Invalid config: {path:?}, reverting to defaults. Error: {err:?}",);
                    Self::default()
                })
        } else {
            log::info!("Loading default configuration");
            Self::default()
        };

        config.input_map = InputMap::from_bindings(&config.input_bindings);
        let region = config.control_deck.region;
        Self::set_region(&mut config, region);

        config
    }

    pub fn set_binding(&mut self, input: Input, slot: Player, action: Action) {
        self.input_bindings.push((input, slot, action));
        self.input_map.insert(input, (slot, action));
    }

    pub fn unset_binding(&mut self, input: Input) {
        self.input_bindings.retain(|(i, ..)| i != &input);
        self.input_map.remove(&input);
    }

    pub fn set_region(&mut self, region: NesRegion) {
        match region {
            NesRegion::Ntsc => self.frame_rate = 60.0,
            NesRegion::Pal => self.frame_rate = 50.0,
            NesRegion::Dendy => self.frame_rate = 59.0,
        }
        self.target_frame_duration = Duration::from_secs_f64(self.frame_rate.recip());
        log::info!(
            "Updated frame rate based on NES Region: {region:?} ({:?}Hz)",
            self.frame_rate,
        );
    }

    #[inline]
    #[must_use]
    pub fn get_dimensions(&self) -> (u32, u32) {
        let (width, height) = match self.control_deck.region {
            NesRegion::Ntsc => (WINDOW_WIDTH_NTSC, WINDOW_HEIGHT_NTSC),
            NesRegion::Pal | NesRegion::Dendy => (WINDOW_WIDTH_PAL, WINDOW_HEIGHT_PAL),
        };
        ((self.scale * width) as u32, (self.scale * height) as u32)
    }

    pub fn directory() -> PathBuf {
        control_deck::Config::directory()
    }

    #[must_use]
    pub(crate) fn path<P: AsRef<std::path::Path>>(path: P) -> PathBuf {
        Self::directory().join(path)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save(&self) {
        // TODO
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self) -> crate::NesResult<()> {
        use anyhow::Context;
        use std::fs::{self, File};

        if *self == Config::default() || !self.control_deck.save_on_exit {
            // Don't save default configuration
            return Ok(());
        }

        let dir = Self::directory();
        if !dir.exists() {
            fs::create_dir_all(&dir).with_context(|| {
                format!("failed to create config directory: {}", dir.display(),)
            })?;
            log::info!("created config directory: {}", dir.display());
        }

        let path = Self::path(Self::FILENAME);
        File::create(&path)
            .with_context(|| format!("failed to create config file: {path:?}"))
            .and_then(|file| {
                serde_json::to_writer_pretty(file, &self).context("failed to serialize config")
            })?;
        log::info!("Saved configuration");
        Ok(())
    }
}

impl Nes {
    #[cfg(target_arch = "wasm32")]
    pub fn save_config(&mut self) {
        // TODO: Save to local storage
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.config.scale = scale;
        // TODO: switch to egui
        // let (font_size, fpad, ipad) = match scale as usize {
        //     1 => (6, 2, 2),
        //     2 => (8, 6, 4),
        //     3 => (12, 8, 6),
        //     _ => (16, 10, 8),
        // };
        // s.font_size(font_size).expect("valid font size");
        // s.theme_mut().spacing.frame_pad = point!(fpad, fpad);
        // s.theme_mut().spacing.item_pad = point!(ipad, ipad);
    }

    pub fn change_speed(&mut self, delta: f32) {
        self.config.frame_speed = (self.config.frame_speed + delta).clamp(MIN_SPEED, MAX_SPEED);
        self.set_speed(self.config.frame_speed);
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.config.frame_speed = speed;
        self.send_event(DeckEvent::SetFrameSpeed(self.config.frame_speed));
    }
}
