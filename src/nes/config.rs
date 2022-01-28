use crate::{
    nes::{event::InputBindings, Mode, Nes},
    NesResult,
};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

const KEYBINDS: &str = "./config/keybinds.json";
const DEFAULT_SPEED: f32 = 1.0; // 100% - 60 Hz
const MIN_SPEED: f32 = 0.1; // 10% - 6 Hz
const MAX_SPEED: f32 = 4.0; // 400% - 240 Hz

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Settings {
    pub(crate) pause_in_bg: bool,
    pub(crate) sound: bool,
    pub(crate) fullscreen: bool,
    pub(crate) vsync: bool,
    pub(crate) concurrent_dpad: bool,
    pub(crate) consistent_ram: bool,
    pub(crate) save_slot: u8,
    pub(crate) scale: f32,
    pub(crate) speed: f32,
}

#[derive(Default, Debug, Clone)]
/// NES emulation configuration settings.
pub(crate) struct Config {
    pub(crate) rom_path: PathBuf,
    pub(crate) pause_in_bg: bool,
    pub(crate) sound: bool,
    pub(crate) fullscreen: bool,
    pub(crate) vsync: bool,
    pub(crate) concurrent_dpad: bool,
    pub(crate) consistent_ram: bool,
    pub(crate) save_slot: u8,
    pub(crate) scale: f32,
    pub(crate) speed: f32,
    pub(crate) input_bindings: InputBindings,
    pub(crate) genie_codes: Vec<String>,
    // TODO: Runtime log level
}

impl Config {
    pub(crate) fn new() -> NesResult<Self> {
        Ok(Self {
            rom_path: env::current_dir().unwrap_or_default(),
            pause_in_bg: true,
            sound: true,
            fullscreen: false,
            vsync: true,
            concurrent_dpad: false,
            consistent_ram: false,
            save_slot: 1,
            scale: 3.0,
            speed: 1.0,
            input_bindings: InputBindings::from_file(KEYBINDS)?,
            genie_codes: vec![],
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> NesResult<Self> {
        let path = path.as_ref();
        let file = BufReader::new(File::open(path)?);

        let settings: Settings = serde_json::from_reader(file)
            .with_context(|| format!("Failed to parse `{}`", path.display()))?;

        Ok(Self {
            pause_in_bg: settings.pause_in_bg,
            sound: settings.sound,
            fullscreen: settings.fullscreen,
            vsync: settings.vsync,
            concurrent_dpad: settings.concurrent_dpad,
            consistent_ram: settings.consistent_ram,
            save_slot: settings.save_slot,
            scale: settings.scale,
            speed: settings.speed,
            ..Config::new()?
        })
    }
}

impl Nes {
    pub(crate) fn change_speed(&mut self, delta: f32) {
        if let Mode::Recording | Mode::Replaying = self.mode {
            self.add_message("Speed changes disabled while recording or replaying");
            return;
        }
        if self.config.speed % 0.25 != 0.0 {
            // Round to nearest quarter
            self.config.speed = (self.config.speed * 4.0).floor() / 4.0;
        }
        self.config.speed += DEFAULT_SPEED * delta;
        if self.config.speed < MIN_SPEED {
            self.config.speed = MIN_SPEED;
        } else if self.config.speed > MAX_SPEED {
            self.config.speed = MAX_SPEED;
        }
        self.control_deck.set_speed(self.config.speed);
    }

    pub(crate) fn set_speed(&mut self, speed: f32) {
        if let Mode::Recording | Mode::Replaying = self.mode {
            self.add_message("Speed changes disabled while recording or replaying");
            return;
        }
        self.config.speed = speed;
        self.control_deck.set_speed(self.config.speed);
    }
}
