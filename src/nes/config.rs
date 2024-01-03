use crate::{
    common::NesRegion,
    input::FourPlayer,
    mem::RamState,
    nes::{
        event::{Action, Input, InputBindings, InputMapping},
        Nes,
    },
    ppu::Ppu,
    video::VideoFilter,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

pub const WINDOW_TITLE: &str = "TetaNES";
const MIN_SPEED: f32 = 0.25; // 25% - 15 Hz
const MAX_SPEED: f32 = 2.0; // 200% - 120 Hz
const WINDOW_WIDTH_NTSC: f32 = Ppu::WIDTH as f32 * 8.0 / 7.0 + 0.5; // for 8:7 Aspect Ratio
const WINDOW_WIDTH_PAL: f32 = Ppu::WIDTH as f32 * 18.0 / 13.0 + 0.5; // for 18:13 Aspect Ratio
const WINDOW_HEIGHT_NTSC: f32 = Ppu::HEIGHT as f32 - 16.0; // NTSC trims top bottom and 8 scanlines
const WINDOW_HEIGHT_PAL: f32 = Ppu::HEIGHT as f32;
pub const FRAME_TRIM_PITCH: usize = (4 * Ppu::WIDTH * 8) as usize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
#[serde(default)] // Ensures new fields don't break existing configurations
/// NES emulation configuration settings.
pub struct Config {
    pub rom_path: PathBuf,
    pub show_hidden_files: bool,
    pub pause_in_bg: bool,
    pub audio_enabled: bool,
    pub debug: bool,
    pub save_on_exit: bool,
    pub fullscreen: bool,
    pub vsync: bool,
    pub filter: VideoFilter,
    pub concurrent_dpad: bool,
    pub region: NesRegion,
    pub frame_rate: f64,
    #[serde(skip)]
    pub target_frame_time: Duration,
    pub ram_state: RamState,
    pub save_slot: u8,
    pub scale: f32,
    pub speed: f32,
    pub replay_path: Option<PathBuf>,
    pub rewind: bool,
    pub rewind_frames: u32,
    pub rewind_buffer_size: usize,
    pub four_player: FourPlayer,
    pub zapper: bool,
    pub controller_deadzone: f64,
    pub audio_sample_rate: f32,
    pub audio_latency: Duration,
    pub genie_codes: Vec<String>,
    pub bindings: InputBindings,
    #[serde(skip)]
    pub input_map: InputMapping,
}

impl Config {
    #[cfg(not(target_arch = "wasm32"))]
    const FILENAME: &'static str = "config.json";

    #[cfg(target_arch = "wasm32")]
    pub fn load() -> Self {
        // TODO: Load from local storage?
        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load() -> Self {
        use crate::common::config_path;
        use anyhow::Context;
        use std::fs::File;

        let config_path = config_path(Self::FILENAME);
        let mut config = if config_path.exists() {
            File::open(&config_path)
                .with_context(|| format!("failed to open {config_path:?}"))
                .and_then(|file| Ok(serde_json::from_reader::<_, Config>(file)?))
                .with_context(|| format!("failed to parse {config_path:?}"))
                .unwrap_or_else(|err| {
                    log::error!(
                        "Invalid config: {config_path:?}, reverting to defaults. Error: {err:?}",
                    );
                    Self::default()
                })
        } else {
            Self::default()
        };

        config.input_map = InputMapping::from_bindings(&config.bindings);

        config
    }

    pub fn set_binding(&mut self, input: Input, action: Action) {
        self.input_map.insert(input, action);
        self.bindings.set(input, action);
    }

    pub fn unset_binding(&mut self, input: Input) {
        self.input_map.remove(&input);
        self.bindings.unset(input);
    }

    #[inline]
    #[must_use]
    pub fn get_dimensions(&self) -> (u32, u32) {
        let (width, height) = match self.region {
            NesRegion::Ntsc => (WINDOW_WIDTH_NTSC, WINDOW_HEIGHT_NTSC),
            NesRegion::Pal | NesRegion::Dendy => (WINDOW_WIDTH_PAL, WINDOW_HEIGHT_PAL),
        };
        ((self.scale * width) as u32, (self.scale * height) as u32)
    }
}

impl Nes {
    #[cfg(target_arch = "wasm32")]
    pub fn save_config(&mut self) {
        // TODO: Save to local storage
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_config(&mut self) {
        use crate::common::{config_dir, config_path};
        use anyhow::Context;
        use std::fs::{self, File};

        let config_dir = config_dir();
        if !config_dir.exists() {
            if let Err(err) =
                fs::create_dir_all(config_dir).context("failed to create config directory")
            {
                log::error!("{:?}", err);
            }
        }

        let path = config_path(Config::FILENAME);
        match File::create(&path)
            .with_context(|| format!("failed to open {path:?}"))
            .and_then(|file| {
                serde_json::to_writer_pretty(file, &self.config)
                    .context("failed to serialize config")
            }) {
            Ok(_) => log::info!("Saved configuration"),
            Err(err) => {
                log::error!("{:?}", err);
                self.add_message("Failed to save configuration");
            }
        }
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
        self.config.speed = (self.config.speed + delta).clamp(MIN_SPEED, MAX_SPEED);
        self.set_speed(self.config.speed);
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.config.speed = speed;
        if let Err(err) = self
            .audio
            .set_output_frequency(self.config.audio_sample_rate / speed)
        {
            log::error!("failed to set speed to {speed}: {err:?}");
        }
    }

    /// Updates the frame rate and vsync settings based on NES region.
    pub fn update_frame_rate(&mut self) {
        match self.config.region {
            NesRegion::Ntsc => self.config.frame_rate = 60.0,
            NesRegion::Pal => self.config.frame_rate = 50.0,
            NesRegion::Dendy => self.config.frame_rate = 59.0,
        }
        self.config.target_frame_time = Duration::from_secs_f64(self.config.frame_rate.recip());
        log::debug!(
            "Updated NES Region and emulated frame rate: {:?} ({:?}Hz)",
            self.config.region,
            self.config.frame_rate,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_vsync(&mut self, _enabled: bool) {
        // TODO: feature not released yet: https://github.com/parasyte/pixels/pull/373
        self.add_message("Vsync toggling currently not supported");
        // self.config.vsync = enabled;
        // if self.config.vsync {
        //     use crate::nes::RenderMainMsg;
        //     if let Err(err) = self
        //         .render_main_tx
        //         .send(RenderMainMsg::SetVsync(self.config.vsync))
        //     {
        //         log::error!("failed to send vsync message to render_main: {err:?}");
        //     }
        //     self.add_message("Vsync Enabled");
        // } else {
        //     self.add_message("Vsync Disabled");
        // }
    }
}

impl Default for Config {
    fn default() -> Self {
        let frame_rate = 60.0;
        let bindings = InputBindings::default();
        let input_map = InputMapping::from_bindings(&bindings);
        Self {
            rom_path: PathBuf::from("./"),
            show_hidden_files: false,
            // Only pause in bg by default in release builds
            pause_in_bg: !cfg!(debug_assertions),
            audio_enabled: true,
            debug: false,
            // Only save by default in release builds
            save_on_exit: !cfg!(debug_assertions),
            fullscreen: false,
            vsync: true,
            filter: VideoFilter::default(),
            concurrent_dpad: false,
            region: NesRegion::default(),
            frame_rate,
            target_frame_time: Duration::from_secs_f64(frame_rate.recip()),
            ram_state: RamState::Random,
            save_slot: 1,
            scale: 3.0,
            speed: 1.0,
            replay_path: None,
            rewind: false,
            rewind_frames: 2,
            rewind_buffer_size: 20,
            four_player: FourPlayer::default(),
            zapper: false,
            controller_deadzone: 0.5,
            audio_sample_rate: 48_000.0,
            audio_latency: Duration::from_millis(if cfg!(target_arch = "wasm32") {
                60
            } else {
                30
            }),
            genie_codes: vec![],
            bindings,
            input_map,
        }
    }
}
