use crate::{
    logging::{LogLevel, Loggable},
    nes::Nes,
    serialization::Savable,
    NesResult,
};
use pix_engine::StateData;
use std::{
    env,
    io::{Read, Write},
};

pub(super) const DEFAULT_SPEED: f32 = 1.0; // 100% - 60 Hz
const MIN_SPEED: f32 = 0.25; // 25% - 240 Hz
const MAX_SPEED: f32 = 2.0; // 200% - 30 Hz

#[derive(Clone)]
pub struct NesConfig {
    pub path: String,
    pub debug: bool,
    pub log_level: LogLevel,
    pub fullscreen: bool,
    pub vsync: bool,
    pub sound_enabled: bool,
    pub record: bool,
    pub replay: Option<String>,
    pub rewind_enabled: bool,
    pub save_enabled: bool,
    pub concurrent_dpad: bool,
    pub randomize_ram: bool,
    pub save_slot: u8,
    pub scale: u32,
    pub speed: f32,
    pub unlock_fps: bool,
    pub genie_codes: Vec<String>,
}

impl NesConfig {
    pub fn new() -> Self {
        let mut config = Self {
            path: String::new(),
            debug: false,
            log_level: LogLevel::default(),
            fullscreen: false,
            vsync: false,
            sound_enabled: true,
            record: false,
            replay: None,
            rewind_enabled: true,
            save_enabled: true,
            concurrent_dpad: false,
            randomize_ram: false,
            save_slot: 1,
            scale: 3,
            speed: 1.0,
            unlock_fps: false,
            genie_codes: Vec::new(),
        };
        if let Some(p) = env::current_dir().unwrap_or_default().to_str() {
            config.path = p.to_string();
        }
        config
    }
}

impl Savable for NesConfig {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        // TODO add path
        // Ignore
        // debug
        // log_level
        self.fullscreen.save(fh)?;
        self.vsync.save(fh)?;
        self.sound_enabled.save(fh)?;
        // Ignore record/replay
        self.rewind_enabled.save(fh)?;
        self.save_enabled.save(fh)?;
        self.concurrent_dpad.save(fh)?;
        self.randomize_ram.save(fh)?;
        self.save_slot.save(fh)?;
        self.scale.save(fh)?;
        self.speed.save(fh)?;
        self.unlock_fps.save(fh)?;
        // Ignore genie_codes
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        // TODO add path
        // Ignore
        // debug
        // log_level
        self.fullscreen.load(fh)?;
        self.vsync.load(fh)?;
        self.sound_enabled.load(fh)?;
        // Ignore record/replay
        self.rewind_enabled.load(fh)?;
        self.save_enabled.load(fh)?;
        self.concurrent_dpad.load(fh)?;
        self.randomize_ram.load(fh)?;
        self.save_slot.load(fh)?;
        self.scale.load(fh)?;
        self.speed.load(fh)?;
        self.unlock_fps.load(fh)?;
        Ok(())
    }
}

impl Nes {
    pub(super) fn change_speed(&mut self, delta: f32) {
        if self.recording {
            self.add_message("Speed changes disabled while recording");
        } else {
            self.config.speed += DEFAULT_SPEED * delta;
            if self.config.speed < MIN_SPEED {
                self.config.speed = MIN_SPEED;
            } else if self.config.speed > MAX_SPEED {
                self.config.speed = MAX_SPEED;
            }
            self.cpu.bus.apu.set_speed(self.config.speed);
        }
    }

    pub(super) fn update_title(&mut self, data: &mut StateData) {
        let mut title = String::new();
        if self.paused {
            title.push_str("Paused");
        } else {
            title.push_str(&format!("Save Slot: {}", self.config.save_slot));
            if self.config.speed != DEFAULT_SPEED {
                title.push_str(&format!(" - Speed: {}%", self.config.speed * 100.0));
            }
        }
        data.set_title(&title);
    }

    pub(super) fn set_log_level(&mut self) {
        let level = self.config.log_level;
        self.cpu.set_log_level(level);
        self.cpu.bus.ppu.set_log_level(level);
        if level > LogLevel::Debug {
            self.config.sound_enabled = false;
        }
    }
}
