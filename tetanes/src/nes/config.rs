use super::{
    action::Action,
    event::{EmulationEvent, Input, InputBinding, InputMap, RendererEvent},
    Nes,
};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tetanes_core::{
    common::NesRegion, control_deck::Config as DeckConfig, input::Player, ppu::Ppu,
};
use tetanes_util::{platform::time::Duration, NesError, NesResult};
use tracing::info;

// https://www.nesdev.org/wiki/Overscan
pub const NTSC_RATIO: f32 = 8.0 / 7.0;
pub const PAL_RATIO: f32 = 18.0 / 13.0;
pub const OVERSCAN_TRIM: usize = (4 * Ppu::WIDTH * 8) as usize;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[must_use]
pub enum Scale {
    X1,
    X2,
    X3,
    X4,
}

impl Default for Scale {
    fn default() -> Self {
        if cfg!(target_arch = "wasm32") {
            Self::X2
        } else {
            Self::X3
        }
    }
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

impl From<&Scale> for f32 {
    fn from(val: &Scale) -> Self {
        Self::from(*val)
    }
}

impl From<Scale> for f64 {
    fn from(val: Scale) -> Self {
        f32::from(val) as f64
    }
}

impl From<&Scale> for f64 {
    fn from(val: &Scale) -> Self {
        Self::from(*val)
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
pub enum FrameSpeed {
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

impl FrameSpeed {
    pub fn increment(&self) -> Self {
        match self {
            FrameSpeed::X25 => FrameSpeed::X50,
            FrameSpeed::X50 => FrameSpeed::X75,
            FrameSpeed::X75 => FrameSpeed::X100,
            FrameSpeed::X100 => FrameSpeed::X125,
            FrameSpeed::X125 => FrameSpeed::X150,
            FrameSpeed::X150 => FrameSpeed::X175,
            FrameSpeed::X175 => FrameSpeed::X200,
            FrameSpeed::X200 => FrameSpeed::X200,
        }
    }

    pub fn decrement(&self) -> Self {
        match self {
            FrameSpeed::X25 => FrameSpeed::X25,
            FrameSpeed::X50 => FrameSpeed::X25,
            FrameSpeed::X75 => FrameSpeed::X50,
            FrameSpeed::X100 => FrameSpeed::X75,
            FrameSpeed::X125 => FrameSpeed::X100,
            FrameSpeed::X150 => FrameSpeed::X125,
            FrameSpeed::X175 => FrameSpeed::X150,
            FrameSpeed::X200 => FrameSpeed::X175,
        }
    }
}

impl From<FrameSpeed> for f32 {
    fn from(speed: FrameSpeed) -> Self {
        match speed {
            FrameSpeed::X25 => 0.25,
            FrameSpeed::X50 => 0.50,
            FrameSpeed::X75 => 0.75,
            FrameSpeed::X100 => 1.0,
            FrameSpeed::X125 => 1.25,
            FrameSpeed::X150 => 1.50,
            FrameSpeed::X175 => 1.75,
            FrameSpeed::X200 => 2.0,
        }
    }
}

impl From<&FrameSpeed> for f32 {
    fn from(speed: &FrameSpeed) -> Self {
        Self::from(*speed)
    }
}

impl TryFrom<f32> for FrameSpeed {
    type Error = NesError;
    fn try_from(val: f32) -> Result<Self, Self::Error> {
        match val {
            0.25 => Ok(FrameSpeed::X25),
            0.50 => Ok(FrameSpeed::X50),
            0.75 => Ok(FrameSpeed::X75),
            1.0 => Ok(FrameSpeed::X100),
            1.25 => Ok(FrameSpeed::X125),
            1.50 => Ok(FrameSpeed::X150),
            1.75 => Ok(FrameSpeed::X175),
            2.0 => Ok(FrameSpeed::X200),
            _ => Err(anyhow!("unsupported speed: {val}")),
        }
    }
}

impl AsRef<str> for FrameSpeed {
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

impl std::fmt::Display for FrameSpeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SampleRate {
    #[default]
    S44,
    S48,
}

impl SampleRate {
    pub const MIN: Self = Self::S44;
    pub const MAX: Self = Self::S48;
}

impl From<SampleRate> for u32 {
    fn from(sample_rate: SampleRate) -> Self {
        match sample_rate {
            SampleRate::S44 => 44100,
            SampleRate::S48 => 48000,
        }
    }
}

impl From<&SampleRate> for u32 {
    fn from(sample_rate: &SampleRate) -> Self {
        Self::from(*sample_rate)
    }
}

impl From<SampleRate> for f32 {
    fn from(sample_rate: SampleRate) -> Self {
        u32::from(sample_rate) as f32
    }
}

impl From<&SampleRate> for f32 {
    fn from(sample_rate: &SampleRate) -> Self {
        Self::from(*sample_rate)
    }
}

impl TryFrom<u32> for SampleRate {
    type Error = NesError;
    fn try_from(val: u32) -> Result<Self, Self::Error> {
        match val {
            44100 => Ok(Self::S44),
            48000 => Ok(Self::S48),
            _ => Err(anyhow!("unsupported sample rate: {val}")),
        }
    }
}

impl AsRef<str> for SampleRate {
    fn as_ref(&self) -> &str {
        match self {
            Self::S44 => "44.1 kHz",
            Self::S48 => "48 kHz",
        }
    }
}

impl std::fmt::Display for SampleRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrameRate {
    X50,
    X59,
    #[default]
    X60,
}

impl FrameRate {
    pub const MIN: Self = Self::X50;
    pub const MAX: Self = Self::X60;

    pub fn duration(&self) -> Duration {
        Duration::from_secs_f32(f32::from(self).recip())
    }
}

impl From<FrameRate> for u32 {
    fn from(frame_rate: FrameRate) -> Self {
        match frame_rate {
            FrameRate::X50 => 50,
            FrameRate::X59 => 59,
            FrameRate::X60 => 60,
        }
    }
}

impl From<&FrameRate> for u32 {
    fn from(frame_rate: &FrameRate) -> Self {
        Self::from(*frame_rate)
    }
}

impl From<FrameRate> for f32 {
    fn from(frame_rate: FrameRate) -> Self {
        u32::from(frame_rate) as f32
    }
}

impl From<&FrameRate> for f32 {
    fn from(frame_rate: &FrameRate) -> Self {
        Self::from(*frame_rate)
    }
}

impl TryFrom<u32> for FrameRate {
    type Error = NesError;
    fn try_from(val: u32) -> Result<Self, Self::Error> {
        match val {
            50 => Ok(Self::X50),
            59 => Ok(Self::X59),
            60 => Ok(Self::X60),
            _ => Err(anyhow!("unsupported frame rate: {val}")),
        }
    }
}

impl From<NesRegion> for FrameRate {
    fn from(region: NesRegion) -> Self {
        match region {
            NesRegion::Pal => Self::X50,
            NesRegion::Dendy => Self::X59,
            NesRegion::Ntsc => Self::X60,
        }
    }
}

impl From<&NesRegion> for FrameRate {
    fn from(region: &NesRegion) -> Self {
        Self::from(*region)
    }
}

impl AsRef<str> for FrameRate {
    fn as_ref(&self) -> &str {
        match self {
            Self::X50 => "50 Hz",
            Self::X59 => "59 Hz",
            Self::X60 => "60 Hz",
        }
    }
}

impl std::fmt::Display for FrameRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

/// NES emulation configuration settings.
///
/// # Config JSON
///
/// Configuration for `TetaNES` is stored (by default) in `~/.config/tetanes/config.json`
/// with defaults that can be customized in the `TetaNES` config menu.
///
/// # Bindings
///
/// ## Keyboard Mappings
///
/// A `keys` array with the following values:
///
/// - `controller`: The controller this keybinding should apply to (`One`, `Two`,
///   `Three`, or `Four`).
/// - `key`: A string that maps to a `pix_engine::prelude::Key` variant.
/// - `keymod`: A number that maps to a `pix_engine::prelude::KeyMod` constant:
///   - `NONE`: `-1`
///   - `SHIFT`: `0`,
///   - `CTRL`: `63`,
///   - `ALT`: `255`,
///   - `GUI`: `1023`,
/// - `action`: An object that maps to an `nes::Action` variant. e.g.
///   `{ "Joypad": "Left" } }`
///
/// ## Mouse Mappings
///
/// A `mouse` array with the following values:
///
/// - `controller`: The controller this button should apply to (`One`, `Two`,
///   `Three`, or `Four`).
/// - `button`: A string that maps to a `pix_engine::prelud::Mouse` variant.
/// - `action`: An object that maps to an `Nes::Action` variant. e.g.
///   `{ "Zapper": [0, 0] }`
///
/// ## Controller Button Mappings
///
/// A `buttons` array with the following values:
///
/// - `controller`: The controller this button should apply to (`One`, `Two`,
///   `Three`, or `Four`).
/// - `button`: A string that maps to a `pix_engine::prelude::ControllerButton`
///   variant.
/// - `action`: An object that maps to an `nes::Action` variant. e.g.
///   `{ "Nes": ToggleMenu" } }`
///
/// ## Controller Axis Mappings
///
/// A `axes` array with the following values:
///
/// - `controller`: The controller this button should apply to (`One`, `Two`,
///   `Three`, or `Four`).
/// - `axis`: A string that maps to a `pix_engine::prelude::Axis` variant.
/// - `direction`: `None`, `Positive`, or `Negative` to indicate axis direction.
/// - `action`: An object that maps to an `nes::Action` variant. e.g.
///   `{ "Feature": "SaveState" } }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
#[serde(default)] // Ensures new fields don't break existing configurations
pub struct Config {
    pub aspect_ratio: f32,
    pub audio_enabled: bool,
    pub audio_buffer_size: usize,
    pub audio_latency: Duration,
    pub audio_sample_rate: SampleRate,
    pub concurrent_dpad: bool,
    pub controller_deadzone: f64,
    pub debug: bool,
    pub deck: DeckConfig,
    pub frame_rate: FrameRate,
    pub frame_speed: FrameSpeed,
    pub fullscreen: bool,
    pub hide_overscan: bool,
    pub input_bindings: Vec<InputBinding>,
    #[serde(skip)]
    pub input_map: InputMap,
    pub replay_path: Option<PathBuf>,
    pub rewind: bool,
    pub rewind_buffer_size_mb: usize,
    pub rewind_interval: u8,
    pub rom_path: PathBuf,
    pub scale: Scale,
    pub show_fps: bool,
    pub show_hidden_files: bool,
    pub show_messages: bool,
    #[serde(skip)]
    pub target_frame_duration: Duration,
    pub threaded: bool,
    pub vsync: bool,
}

impl Default for Config {
    fn default() -> Self {
        let frame_rate = FrameRate::default();
        let input_map = InputMap::default();
        let input_bindings = input_map
            .iter()
            .map(|(input, (slot, action))| (*input, *slot, *action))
            .collect();
        let deck = DeckConfig::default();
        let aspect_ratio = match deck.region {
            NesRegion::Ntsc => NTSC_RATIO,
            NesRegion::Pal | NesRegion::Dendy => PAL_RATIO,
        };
        Self {
            aspect_ratio,
            audio_buffer_size: if cfg!(target_arch = "wasm32") {
                // Too low a value for wasm causes audio underruns in Chrome
                2048
            } else {
                512
            },
            audio_latency: Duration::from_millis(50),
            audio_enabled: true,
            audio_sample_rate: SampleRate::default(),
            concurrent_dpad: false,
            controller_deadzone: 0.5,
            debug: false,
            deck,
            frame_rate,
            frame_speed: FrameSpeed::default(),
            fullscreen: false,
            hide_overscan: true,
            input_map,
            input_bindings,
            replay_path: None,
            rewind: false,
            rewind_buffer_size_mb: 20 * 1024 * 1024,
            rewind_interval: 2,
            rom_path: PathBuf::from("./"),
            scale: Scale::default(),
            show_fps: cfg!(debug_assertions),
            show_hidden_files: false,
            show_messages: true,
            target_frame_duration: frame_rate.duration(),
            threaded: true,
            vsync: true,
        }
    }
}

impl From<Config> for DeckConfig {
    fn from(config: Config) -> Self {
        config.deck
    }
}

impl Config {
    pub const WINDOW_TITLE: &'static str = "TetaNES";
    pub const DEFAULT_DIRECTORY: &'static str = DeckConfig::DIR;
    pub const FILENAME: &'static str = "config.json";

    #[cfg(target_arch = "wasm32")]
    pub fn save(&self) -> NesResult<()> {
        // TODO
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self) -> NesResult<()> {
        use anyhow::Context;
        use std::fs::{self, File};
        use tracing::info;

        if !self.deck.save_on_exit {
            return Ok(());
        }

        if !self.deck.dir.exists() {
            fs::create_dir_all(&self.deck.dir).with_context(|| {
                format!(
                    "failed to create config directory: {}",
                    self.deck.dir.display()
                )
            })?;
            info!("created config directory: {}", self.deck.dir.display());
        }

        let path = self.deck.dir.join(Self::FILENAME);
        File::create(&path)
            .with_context(|| format!("failed to create config file: {path:?}"))
            .and_then(|file| {
                serde_json::to_writer_pretty(file, &self).context("failed to serialize config")
            })?;
        info!("Saved configuration");
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load() -> Self {
        info!("Loading default configuration");
        // TODO: Load from local storage?
        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: Option<PathBuf>) -> Self {
        use anyhow::Context;
        use std::fs::File;
        use tracing::{error, info};

        let path = path.unwrap_or_else(|| DeckConfig::default_dir().join(Self::FILENAME));
        let mut config = if path.exists() {
            info!("Loading saved configuration");
            File::open(&path)
                .with_context(|| format!("failed to open {path:?}"))
                .and_then(|file| Ok(serde_json::from_reader::<_, Config>(file)?))
                .with_context(|| format!("failed to parse {path:?}"))
                .unwrap_or_else(|err| {
                    error!("Invalid config: {path:?}, reverting to defaults. Error: {err:?}",);
                    Self::default()
                })
        } else {
            info!("Loading default configuration");
            Self::default()
        };

        config.input_map = InputMap::from_bindings(&config.input_bindings);
        let region = config.deck.region;
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
        self.frame_rate = FrameRate::from(region);
        self.aspect_ratio = match region {
            NesRegion::Ntsc => NTSC_RATIO,
            NesRegion::Pal => PAL_RATIO,
            NesRegion::Dendy => PAL_RATIO,
        };
        self.target_frame_duration = Duration::from_secs_f32(
            (f32::from(self.frame_rate) * f32::from(self.frame_speed)).recip(),
        );
        info!(
            "Updated frame rate based on NES Region: {region:?} ({:?}Hz)",
            self.frame_rate,
        );
    }

    pub fn set_frame_speed(&mut self, speed: FrameSpeed) {
        self.frame_speed = speed;
        self.target_frame_duration = Duration::from_secs_f32(
            (f32::from(self.frame_rate) * f32::from(self.frame_speed)).recip(),
        );
    }

    #[must_use]
    pub fn window_dimensions(&self) -> (f32, f32) {
        let scale = f32::from(self.scale);
        let (width, height) = self.texture_dimensions();
        (scale * width * self.aspect_ratio, scale * height)
    }

    #[must_use]
    pub fn texture_dimensions(&self) -> (f32, f32) {
        let width = Ppu::WIDTH;
        let height = if self.hide_overscan {
            Ppu::HEIGHT - 16
        } else {
            Ppu::HEIGHT
        };
        (width as f32, height as f32)
    }
}

impl Nes {
    pub fn set_region(&mut self, region: NesRegion) {
        self.config.set_region(region);
        self.trigger_event(EmulationEvent::SetRegion(region));
        self.trigger_event(EmulationEvent::SetTargetFrameDuration(
            self.config.target_frame_duration,
        ));
        self.add_message(format!("Changed NES Region to {region:?}"));
    }

    pub fn set_scale(&mut self, scale: Scale) {
        self.config.scale = scale;
        self.trigger_event(RendererEvent::SetScale(scale));
        self.add_message(format!("Changed Scale to {scale}"));
    }

    pub fn set_speed(&mut self, speed: FrameSpeed) {
        self.config.set_frame_speed(speed);
        self.trigger_event(EmulationEvent::SetFrameSpeed(speed));
        self.trigger_event(EmulationEvent::SetTargetFrameDuration(
            self.config.target_frame_duration,
        ));
        self.add_message(format!("Changed Emulation Speed to {speed}"));
    }
}
