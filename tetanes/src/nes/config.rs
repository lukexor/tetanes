use crate::nes::{
    action::Action,
    event::EmulationEvent,
    input::{Input, InputBinding, InputMap},
    Nes,
};
use anyhow::Context;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf, sync::Arc};
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
    pub buffer_size: usize,
    pub enabled: bool,
    pub latency: Duration,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            buffer_size: if cfg!(target_arch = "wasm32") {
                // Too low a value for wasm causes audio underruns in Chrome
                2048
            } else {
                512
            },
            enabled: true,
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
    pub cycle_accurate: bool,
    pub load_on_start: bool,
    pub rewind: bool,
    pub save_on_exit: bool,
    pub save_slot: u8,
    pub speed: f32,
    pub run_ahead: usize,
    pub threaded: bool,
}

impl Default for EmulationConfig {
    fn default() -> Self {
        Self {
            cycle_accurate: true,
            load_on_start: true,
            rewind: true,
            save_on_exit: true,
            save_slot: 1,
            speed: 1.0,
            // FIXME debug builds aren't currently fast enough to default to 1 without audio
            // underruns.
            run_ahead: if cfg!(debug_assertions) { 0 } else { 1 },
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
    pub bindings: Vec<InputBinding>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            controller_deadzone: 0.5,
            bindings: InputMap::default()
                .iter()
                .map(|(input, (slot, action))| (*input, *slot, *action))
                .collect(),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct Config(Arc<RwLock<ConfigImpl>>);

impl Config {
    pub const SAVE_DIR: &'static str = "save";
    pub const WINDOW_TITLE: &'static str = "TetaNES";
    pub const FILENAME: &'static str = "config.json";

    pub fn load(path: Option<PathBuf>) -> Self {
        Self(Arc::new(RwLock::new(ConfigImpl::load(path))))
    }

    pub fn read<R>(&self, reader: impl FnOnce(&ConfigImpl) -> R) -> R {
        reader(&self.0.read())
    }

    pub fn write<R>(&self, writer: impl FnOnce(&mut ConfigImpl) -> R) -> R {
        writer(&mut self.0.write())
    }

    #[must_use]
    pub fn config_dir() -> Option<PathBuf> {
        dirs::config_local_dir().map(|dir| dir.join(DeckConfig::BASE_DIR))
    }

    #[must_use]
    pub fn data_dir() -> Option<PathBuf> {
        dirs::data_local_dir().map(|dir| dir.join(DeckConfig::BASE_DIR))
    }

    #[must_use]
    pub fn document_dir() -> Option<PathBuf> {
        dirs::document_dir().map(|dir| dir.join(DeckConfig::BASE_DIR))
    }

    #[must_use]
    pub fn picture_dir() -> Option<PathBuf> {
        dirs::picture_dir().map(|dir| dir.join(DeckConfig::BASE_DIR))
    }

    #[must_use]
    pub fn audio_dir() -> Option<PathBuf> {
        dirs::audio_dir().map(|dir| dir.join(DeckConfig::BASE_DIR))
    }

    #[must_use]
    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|dir| dir.join(Self::FILENAME))
    }

    #[must_use]
    pub fn save_path(name: &str, slot: u8) -> Option<PathBuf> {
        Self::data_dir().map(|dir| {
            dir.join(Self::SAVE_DIR)
                .join(name)
                .join(format!("slot-{}", slot))
                .with_extension("sav")
        })
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
pub struct ConfigImpl {
    pub deck: DeckConfig,
    pub emulation: EmulationConfig,
    pub audio: AudioConfig,
    pub renderer: RendererConfig,
    pub input: InputConfig,
}

impl ConfigImpl {
    pub fn save(&self) -> anyhow::Result<()> {
        if !self.emulation.save_on_exit {
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

    pub fn set_binding(&mut self, input: Input, slot: Player, action: Action) {
        self.input.bindings.push((input, slot, action));
    }

    pub fn unset_binding(&mut self, input: Input) {
        self.input.bindings.retain(|(i, ..)| i != &input);
    }

    pub fn set_emulation_speed(&mut self, speed: f32) {
        self.emulation.speed = speed;
    }

    #[must_use]
    pub fn window_size(&self) -> LogicalSize<f32> {
        let aspect_ratio = self.deck.region.aspect_ratio();
        let scale = self.renderer.scale;
        let texture_size = self.texture_size();
        LogicalSize::new(
            scale * texture_size.width as f32 * aspect_ratio,
            scale * texture_size.height as f32,
        )
    }

    #[must_use]
    pub fn texture_size(&self) -> LogicalSize<u32> {
        let width = Ppu::WIDTH;
        let height = if self.renderer.hide_overscan {
            Ppu::HEIGHT - 16
        } else {
            Ppu::HEIGHT
        };
        LogicalSize::new(width, height)
    }
}

impl Nes {
    pub fn set_region(&mut self, region: NesRegion) {
        self.trigger_event(EmulationEvent::SetRegion(region));
        self.add_message(format!("Changed NES Region to {region:?}"));
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.config.write(|cfg| cfg.set_emulation_speed(speed));
        self.trigger_event(EmulationEvent::SetSpeed(speed));
        self.add_message(format!("Changed Emulation Speed to {speed}"));
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
