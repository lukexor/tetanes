use crate::nes::input::{ActionBindings, Input};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};
use tetanes_core::{
    common::NesRegion, control_deck::Config as DeckConfig, fs, input::Player, ppu::Ppu,
    time::Duration,
};
use tracing::{error, info};
use winit::dpi::LogicalSize;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
#[serde(default)] // Ensures new fields don't break existing configurations
pub struct AudioConfig {
    pub enabled: bool,
    pub buffer_size: usize,
    pub latency: Duration,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            buffer_size: if cfg!(target_arch = "wasm32") {
                // Too low a value for wasm causes audio underruns in Chrome
                2048
            } else {
                512
            },
            latency: if cfg!(target_arch = "wasm32") {
                Duration::from_millis(80)
            } else {
                Duration::from_millis(50)
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
#[serde(default)] // Ensures new fields don't break existing configurations
pub struct EmulationConfig {
    pub auto_load: bool,
    pub auto_save: bool,
    pub rewind: bool,
    pub run_ahead: usize,
    pub save_slot: u8,
    pub speed: f32,
    pub threaded: bool,
}

impl Default for EmulationConfig {
    fn default() -> Self {
        Self {
            auto_load: true,
            auto_save: true,
            // WASM framerates suffer with garbage collection pauses when rewind is enabled.
            // FIXME: Perhaps re-using Vec allocations could help resolve it.
            rewind: cfg!(not(target_arch = "wasm32")),
            // WASM struggles to run fast enough with run-ahead and low latency is not needed in
            // debug builds.
            run_ahead: if cfg!(any(debug_assertions, target_arch = "wasm32")) {
                0
            } else {
                1
            },
            save_slot: 1,
            speed: 1.0,
            threaded: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
#[serde(default)] // Ensures new fields don't break existing configurations
pub struct RendererConfig {
    pub fullscreen: bool,
    pub hide_overscan: bool,
    pub scale: f32,
    pub recent_roms: HashSet<PathBuf>,
    pub roms_path: Option<PathBuf>,
    pub show_fps: bool,
    pub show_perf_stats: bool,
    pub show_messages: bool,
    pub show_menubar: bool,
    pub vsync: bool,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            fullscreen: false,
            hide_overscan: true,
            scale: if cfg!(target_arch = "wasm32") {
                2.0
            } else {
                3.0
            },
            recent_roms: HashSet::new(),
            roms_path: None,
            show_fps: cfg!(debug_assertions),
            show_perf_stats: cfg!(debug_assertions),
            show_messages: true,
            show_menubar: true,
            vsync: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
#[serde(default)] // Ensures new fields don't break existing configurations
pub struct InputConfig {
    pub controller_deadzone: f64,
    pub shortcuts: Vec<ActionBindings>,
    pub joypad_bindings: [Vec<ActionBindings>; 4],
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            controller_deadzone: 0.5,
            shortcuts: ActionBindings::default_shortcuts(),
            joypad_bindings: [Player::One, Player::Two, Player::Three, Player::Four]
                .map(ActionBindings::default_player_bindings),
        }
    }
}

impl InputConfig {
    pub fn clear_binding(&mut self, input: Input) {
        if let Some(binding) = self
            .shortcuts
            .iter_mut()
            .chain(self.joypad_bindings.iter_mut().flatten())
            .flat_map(|bind| &mut bind.bindings)
            .find(|binding| **binding == Some(input))
        {
            *binding = None;
        }
    }
}

/// NES emulation configuration settings.
///
/// # Config JSON
///
/// Configuration for `TetaNES` is stored (by default) in `~/.config/tetanes/config.json`
/// with defaults that can be customized in the `TetaNES` config menu.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
#[serde(default)] // Ensures new fields don't break existing configurations
pub struct Config {
    pub deck: DeckConfig,
    pub emulation: EmulationConfig,
    pub audio: AudioConfig,
    pub renderer: RendererConfig,
    pub input: InputConfig,
}

impl Config {
    pub const SAVE_DIR: &'static str = "save";
    pub const WINDOW_TITLE: &'static str = "TetaNES";
    pub const FILENAME: &'static str = "config.json";

    #[must_use]
    pub fn default_config_dir() -> Option<PathBuf> {
        dirs::config_local_dir().map(|dir| dir.join(DeckConfig::BASE_DIR))
    }

    #[must_use]
    pub fn default_data_dir() -> Option<PathBuf> {
        dirs::data_local_dir().map(|dir| dir.join(DeckConfig::BASE_DIR))
    }

    #[must_use]
    pub fn default_picture_dir() -> Option<PathBuf> {
        dirs::picture_dir().map(|dir| dir.join(DeckConfig::BASE_DIR))
    }

    #[must_use]
    pub fn default_audio_dir() -> Option<PathBuf> {
        dirs::audio_dir().map(|dir| dir.join(DeckConfig::BASE_DIR))
    }

    #[must_use]
    pub fn config_path() -> Option<PathBuf> {
        Self::default_config_dir().map(|dir| dir.join(Self::FILENAME))
    }

    #[must_use]
    pub fn save_path(name: &str, slot: u8) -> Option<PathBuf> {
        Self::default_data_dir().map(|dir| {
            dir.join(Self::SAVE_DIR)
                .join(name)
                .join(format!("slot-{}", slot))
                .with_extension("sav")
        })
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if !self.emulation.auto_save {
            return Ok(());
        }

        if let Some(path) = Config::config_path() {
            let data = serde_json::to_vec_pretty(&self).context("failed to serialize config")?;
            fs::save_raw(path, &data).context("failed to save config")?;
            info!("Saved configuration");
        }

        Ok(())
    }

    pub fn load(path: Option<PathBuf>) -> Self {
        path.or_else(Config::config_path)
            .and_then(|path| {
                path.exists().then(|| {
                    info!("Loading saved configuration");
                    fs::load_raw(&path)
                        .context("failed to load config")
                        .and_then(|data| Ok(serde_json::from_slice::<Self>(&data)?))
                        .with_context(|| format!("failed to parse {path:?}"))
                        .unwrap_or_else(|err| {
                            error!(
                                "Invalid config: {path:?}, reverting to defaults. Error: {err:?}",
                            );
                            Self::default()
                        })
                })
            })
            .unwrap_or_else(|| {
                info!("Loading default configuration");
                Self::default()
            })
    }

    pub fn increment_speed(&mut self) -> f32 {
        if self.emulation.speed <= 1.75 {
            self.emulation.speed += 0.25;
        }
        self.emulation.speed
    }

    pub fn decrement_speed(&mut self) -> f32 {
        if self.emulation.speed >= 0.50 {
            self.emulation.speed -= 0.25;
        }
        self.emulation.speed
    }

    pub fn increment_scale(&mut self) -> f32 {
        if self.renderer.scale <= 4.0 {
            self.renderer.scale += 1.0;
        }
        self.renderer.scale
    }

    pub fn decrement_scale(&mut self) -> f32 {
        if self.renderer.scale >= 2.0 {
            self.renderer.scale -= 1.0;
        }
        self.renderer.scale
    }

    #[must_use]
    pub fn window_size(&self) -> LogicalSize<f32> {
        let scale = self.renderer.scale;
        let texture_size = self.texture_size();
        LogicalSize::new(
            scale * texture_size.width as f32,
            scale * texture_size.height as f32,
        )
    }

    #[must_use]
    pub const fn texture_size(&self) -> LogicalSize<u32> {
        let width = Ppu::WIDTH;
        let height = if self.renderer.hide_overscan {
            Ppu::HEIGHT - 16
        } else {
            Ppu::HEIGHT
        };
        LogicalSize::new(width, height)
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

impl From<NesRegion> for FrameRate {
    fn from(region: NesRegion) -> Self {
        match region {
            NesRegion::Auto | NesRegion::Ntsc => Self::X60,
            NesRegion::Pal => Self::X50,
            NesRegion::Dendy => Self::X59,
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
