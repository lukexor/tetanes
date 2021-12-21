use crate::{
    nes::{event::InputBindings, Mode, Nes},
    NesResult,
};
use std::{env, path::PathBuf};

pub(crate) const SETTINGS: &str = "./config/settings.json";
pub(crate) const INPUT_BINDS: &str = "./config/keybinds.json";

const DEFAULT_SPEED: f32 = 1.0; // 100% - 60 Hz
const MIN_SPEED: f32 = 0.1; // 10% - 6 Hz
const MAX_SPEED: f32 = 4.0; // 400% - 240 Hz

#[derive(Debug, Clone)]
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
            input_bindings: InputBindings::with_config(INPUT_BINDS)?,
            genie_codes: vec![],
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
