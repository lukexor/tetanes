use crate::{
    common::{config_dir, config_path, NesRegion},
    input::FourPlayer,
    mem::RamState,
    nes::{
        event::{Input, InputBindings, InputMapping},
        Nes, WINDOW_HEIGHT, WINDOW_WIDTH_NTSC, WINDOW_WIDTH_PAL,
    },
    video::VideoFilter,
};
use anyhow::Context;
use pix_engine::{
    point,
    prelude::{PixResult, PixState},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::PathBuf,
};

pub(crate) const CONFIG: &str = "config.json";
const DEFAULT_CONFIG: &[u8] = include_bytes!("../../config/config.json");
const MIN_SPEED: f32 = 0.25; // 25% - 15 Hz
const MAX_SPEED: f32 = 2.0; // 200% - 120 Hz

#[derive(Debug, Clone, Serialize, Deserialize)]
/// NES emulation configuration settings.
pub(crate) struct Config {
    pub(crate) rom_path: PathBuf,
    pub(crate) pause_in_bg: bool,
    pub(crate) sound: bool,
    pub(crate) fullscreen: bool,
    pub(crate) vsync: bool,
    pub(crate) filter: VideoFilter,
    pub(crate) concurrent_dpad: bool,
    pub(crate) region: NesRegion,
    pub(crate) ram_state: RamState,
    pub(crate) save_slot: u8,
    pub(crate) scale: f32,
    pub(crate) speed: f32,
    pub(crate) rewind: bool,
    pub(crate) rewind_frames: u32,
    pub(crate) rewind_buffer_size: usize,
    pub(crate) four_player: FourPlayer,
    pub(crate) zapper: bool,
    pub(crate) audio_sample_rate: f32,
    pub(crate) audio_buffer_size: usize,
    pub(crate) dynamic_rate_control: bool,
    pub(crate) dynamic_rate_delta: f32,
    pub(crate) genie_codes: Vec<String>,
    pub(crate) bindings: InputBindings,
    #[serde(skip)]
    pub(crate) input_map: InputMapping,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rom_path: PathBuf::from("./"),
            pause_in_bg: true,
            sound: true,
            fullscreen: false,
            vsync: true,
            filter: VideoFilter::default(),
            concurrent_dpad: false,
            region: NesRegion::default(),
            ram_state: RamState::default(),
            save_slot: 1,
            scale: 3.0,
            speed: 1.0,
            rewind: false,
            rewind_frames: 2,
            rewind_buffer_size: 20,
            four_player: FourPlayer::default(),
            zapper: false,
            audio_sample_rate: 44_100.0,
            audio_buffer_size: 4096,
            dynamic_rate_control: true,
            dynamic_rate_delta: 0.005,
            genie_codes: vec![],
            bindings: InputBindings::default(),
            input_map: InputMapping::default(),
        }
    }
}

impl Config {
    pub(crate) fn load() -> Self {
        let config_dir = config_dir();
        if !config_dir.exists() {
            if let Err(err) =
                fs::create_dir_all(config_dir).context("failed to create config directory")
            {
                log::error!("{:?}", err);
            }
        }
        let config_path = config_path(CONFIG);
        if !config_path.exists() {
            if let Err(err) =
                fs::write(&config_path, DEFAULT_CONFIG).context("failed to create default config")
            {
                log::error!("{:?}", err);
            }
        }

        let mut config = File::open(&config_path)
            .with_context(|| format!("failed to open {config_path:?}"))
            .and_then(|file| Ok(serde_json::from_reader::<_, Config>(BufReader::new(file))?))
            .or_else(|err| {
                log::error!(
                    "Invalid config: {config_path:?}, reverting to defaults. Error: {err:?}",
                );
                serde_json::from_reader(DEFAULT_CONFIG)
            })
            .with_context(|| format!("failed to parse {config_path:?}"))
            .expect("valid configuration");

        for bind in &config.bindings.keys {
            config.input_map.insert(
                Input::Key((bind.player, bind.key, bind.keymod)),
                bind.action,
            );
        }
        for bind in &config.bindings.mouse {
            config
                .input_map
                .insert(Input::Mouse((bind.player, bind.button)), bind.action);
        }
        for bind in &config.bindings.buttons {
            config
                .input_map
                .insert(Input::Button((bind.player, bind.button)), bind.action);
        }
        for bind in &config.bindings.axes {
            config.input_map.insert(
                Input::Axis((bind.player, bind.axis, bind.direction)),
                bind.action,
            );
        }

        config
    }

    // pub(crate) fn add_binding(&mut self, input: Input, action: Action) {
    //     self.input_map.insert(input, action);
    //     self.bindings.update_from_map(&self.input_map);
    // }

    // pub(crate) fn remove_binding(&mut self, input: Input) {
    //     self.input_map.remove(&input);
    //     self.bindings.update_from_map(&self.input_map);
    // }

    pub(crate) fn get_dimensions(&self) -> (u32, u32) {
        let width = match self.region {
            NesRegion::Ntsc => WINDOW_WIDTH_NTSC,
            NesRegion::Pal | NesRegion::Dendy => WINDOW_WIDTH_PAL,
        };
        let width = (self.scale * width) as u32;
        let height = (self.scale * WINDOW_HEIGHT) as u32;
        (width, height)
    }
}

impl Nes {
    pub(crate) fn save_config(&mut self) {
        let path = config_path(CONFIG);
        match File::create(&path)
            .with_context(|| format!("failed to open {path:?}"))
            .and_then(|file| {
                serde_json::to_writer_pretty(BufWriter::new(file), &self.config)
                    .context("failed to serialize config")
            }) {
            Ok(_) => log::info!("Saved configuration"),
            Err(err) => {
                log::error!("{:?}", err);
                self.add_message("Failed to save configuration");
            }
        }
    }

    pub(crate) fn set_scale(&mut self, s: &mut PixState, scale: f32) {
        self.config.scale = scale;
        let (font_size, fpad, ipad) = match scale as usize {
            1 => (6, 2, 2),
            2 => (8, 6, 4),
            3 => (12, 8, 6),
            _ => (16, 10, 8),
        };
        s.font_size(font_size).expect("valid font size");
        s.theme_mut().spacing.frame_pad = point!(fpad, fpad);
        s.theme_mut().spacing.item_pad = point!(ipad, ipad);
    }

    pub(crate) fn change_speed(&mut self, delta: f32) {
        self.config.speed = (self.config.speed + delta).clamp(MIN_SPEED, MAX_SPEED);
        self.set_speed(self.config.speed);
    }

    pub(crate) fn set_speed(&mut self, speed: f32) {
        self.config.speed = speed;
        self.audio
            .set_output_frequency(self.config.audio_sample_rate / self.config.speed);
    }

    pub(crate) fn update_frame_rate(&mut self, s: &mut PixState) -> PixResult<()> {
        match self.config.region {
            NesRegion::Ntsc => s.frame_rate(60),
            NesRegion::Pal => s.frame_rate(50),
            NesRegion::Dendy => s.frame_rate(59),
        }
        log::debug!(
            "Updated NES Region and frame rate: {:?}, {:?}",
            self.config.region,
            s.target_frame_rate()
        );
        // TODO: Should actually check current screen refresh rate here instead of region
        if self.config.vsync && self.config.region != NesRegion::Ntsc {
            s.vsync(false)?;
        }
        Ok(())
    }
}
