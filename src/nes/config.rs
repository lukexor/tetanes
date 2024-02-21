use crate::{
    common::NesRegion,
    control_deck,
    input::Player,
    nes::{
        event::{Action, DeckEvent, Input, InputBinding, InputMap, RendererEvent},
        Nes,
    },
    ppu::Ppu,
    NesError,
};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

const WINDOW_WIDTH_NTSC: f32 = Ppu::WIDTH as f32 * 8.0 / 7.0 + 0.5; // for 8:7 Aspect Ratio
const WINDOW_WIDTH_PAL: f32 = Ppu::WIDTH as f32 * 18.0 / 13.0 + 0.5; // for 18:13 Aspect Ratio
const WINDOW_HEIGHT: f32 = Ppu::HEIGHT as f32;
pub const OVERSCAN_TRIM: usize = (4 * Ppu::WIDTH * 8) as usize;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[must_use]
pub enum Scale {
    X1,
    X2,
    #[default]
    X3,
    X4,
}

impl From<Scale> for f32 {
    fn from(val: Scale) -> Self {
        match val {
            Scale::X1 => 1.0,
            Scale::X2 => 2.0,
            Scale::X3 => 3.0,
            Scale::X4 => 4.0,
        }
    }
}

impl From<Scale> for f64 {
    fn from(val: Scale) -> Self {
        f32::from(val) as f64
    }
}

impl TryFrom<f32> for Scale {
    type Error = NesError;
    fn try_from(val: f32) -> Result<Self, Self::Error> {
        match val {
            1.0 => Ok(Scale::X1),
            2.0 => Ok(Scale::X2),
            3.0 => Ok(Scale::X3),
            4.0 => Ok(Scale::X4),
            _ => Err(anyhow!("unsupported scale: {val}")),
        }
    }
}

impl AsRef<str> for Scale {
    fn as_ref(&self) -> &str {
        match self {
            Self::X1 => "100%",
            Self::X2 => "200%",
            Self::X3 => "300%",
            Self::X4 => "400%",
        }
    }
}

impl std::fmt::Display for Scale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[must_use]
pub enum Speed {
    X25,
    X50,
    X75,
    #[default]
    X100,
    X125,
    X150,
    X175,
    X200,
}

impl Speed {
    pub fn increment(&self) -> Self {
        match self {
            Speed::X25 => Speed::X50,
            Speed::X50 => Speed::X75,
            Speed::X75 => Speed::X100,
            Speed::X100 => Speed::X125,
            Speed::X125 => Speed::X150,
            Speed::X150 => Speed::X175,
            Speed::X175 => Speed::X200,
            Speed::X200 => Speed::X200,
        }
    }

    pub fn decrement(&self) -> Self {
        match self {
            Speed::X25 => Speed::X25,
            Speed::X50 => Speed::X25,
            Speed::X75 => Speed::X50,
            Speed::X100 => Speed::X75,
            Speed::X125 => Speed::X100,
            Speed::X150 => Speed::X125,
            Speed::X175 => Speed::X150,
            Speed::X200 => Speed::X175,
        }
    }
}

impl From<Speed> for f32 {
    fn from(val: Speed) -> Self {
        match val {
            Speed::X25 => 0.25,
            Speed::X50 => 0.50,
            Speed::X75 => 0.75,
            Speed::X100 => 1.0,
            Speed::X125 => 1.25,
            Speed::X150 => 1.50,
            Speed::X175 => 1.75,
            Speed::X200 => 2.0,
        }
    }
}

impl TryFrom<f32> for Speed {
    type Error = NesError;
    fn try_from(val: f32) -> Result<Self, Self::Error> {
        match val {
            0.25 => Ok(Speed::X25),
            0.50 => Ok(Speed::X50),
            0.75 => Ok(Speed::X75),
            1.0 => Ok(Speed::X100),
            1.25 => Ok(Speed::X125),
            1.50 => Ok(Speed::X150),
            1.75 => Ok(Speed::X175),
            2.0 => Ok(Speed::X200),
            _ => Err(anyhow!("unsupported speed: {val}")),
        }
    }
}

impl AsRef<str> for Speed {
    fn as_ref(&self) -> &str {
        match self {
            Self::X25 => "25%",
            Self::X50 => "50%",
            Self::X75 => "75%",
            Self::X100 => "100%",
            Self::X125 => "125%",
            Self::X150 => "150%",
            Self::X175 => "175%",
            Self::X200 => "200%",
        }
    }
}

impl std::fmt::Display for Speed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SampleRate {
    S32,
    S44_1,
    S48,
    S96,
}

impl From<SampleRate> for f32 {
    fn from(val: SampleRate) -> Self {
        match val {
            SampleRate::S32 => 32000.0,
            SampleRate::S44_1 => 44100.0,
            SampleRate::S48 => 48000.0,
            SampleRate::S96 => 96000.0,
        }
    }
}

impl TryFrom<f32> for SampleRate {
    type Error = NesError;
    fn try_from(val: f32) -> Result<Self, Self::Error> {
        match val {
            32000.0 => Ok(Self::S32),
            44100.0 => Ok(Self::S44_1),
            48000.0 => Ok(Self::S48),
            96000.0 => Ok(Self::S96),
            _ => Err(anyhow!("unsupported sample rate: {val}")),
        }
    }
}

impl AsRef<str> for SampleRate {
    fn as_ref(&self) -> &str {
        match self {
            Self::S32 => "32 kHz",
            Self::S44_1 => "44.1 kHz",
            Self::S48 => "48 kHz",
            Self::S96 => "96 kHz",
        }
    }
}

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
    pub scale: Scale,
    pub frame_speed: Speed,
    pub hide_overscan: bool,
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
            && self.hide_overscan == other.hide_overscan
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
            hide_overscan: true,
            target_frame_duration: Duration::from_secs_f64(frame_rate.recip()),
            scale: Scale::default(),
            frame_speed: Speed::default(),
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

    #[must_use]
    pub fn dimensions(&self) -> (f32, f32) {
        let (width, height) = match self.control_deck.region {
            NesRegion::Ntsc => (WINDOW_WIDTH_NTSC, WINDOW_HEIGHT),
            NesRegion::Pal | NesRegion::Dendy => (WINDOW_WIDTH_PAL, WINDOW_HEIGHT),
        };
        let scale = f32::from(self.scale);
        (scale * width, scale * height)
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

        if !self.control_deck.save_on_exit {
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

    pub fn set_scale(&mut self, scale: Scale) {
        self.config.scale = scale;
        self.send_event(RendererEvent::SetScale(self.config.scale));
    }

    pub fn set_speed(&mut self, speed: Speed) {
        self.config.frame_speed = speed;
        self.send_event(DeckEvent::SetFrameSpeed(self.config.frame_speed));
    }
}
