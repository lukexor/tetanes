use crate::{nes::Nes, serialization::Savable, NesResult};
use pix_engine::prelude::*;
use std::{
    env,
    io::{Read, Write},
    path::PathBuf,
};

pub(super) const DEFAULT_SPEED: f32 = 1.0; // 100% - 60 Hz
pub(super) const MIN_SPEED: f32 = 0.10; // 10%
pub(super) const MAX_SPEED: f32 = 4.0; // 400%

#[derive(Clone)]
pub struct NesConfig {
    pub path: PathBuf,
    pub debug: bool,
    pub pause_in_bg: bool,
    pub fullscreen: bool,
    pub vsync: bool,
    pub sound_enabled: bool,
    pub record: bool,
    pub replay: Option<PathBuf>,
    pub rewind_enabled: bool,
    pub save_enabled: bool,
    pub clear_save: bool,
    pub concurrent_dpad: bool,
    pub save_slot: u8,
    pub scale: u32,
    pub speed: f32,
    pub genie_codes: Vec<String>,
}

impl NesConfig {
    pub fn new() -> Self {
        Self {
            path: env::current_dir().unwrap_or_default(),
            debug: false,
            pause_in_bg: true,
            fullscreen: false,
            vsync: false,
            sound_enabled: true,
            record: false,
            replay: None,
            rewind_enabled: true,
            save_enabled: true,
            clear_save: true,
            concurrent_dpad: false,
            save_slot: 1,
            scale: 3,
            speed: 1.0,
            genie_codes: Vec::new(),
        }
    }
}

impl Savable for NesConfig {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        // Ignore
        // debug
        self.pause_in_bg.save(fh)?;
        self.fullscreen.save(fh)?;
        self.vsync.save(fh)?;
        self.sound_enabled.save(fh)?;
        // Ignore record/replay
        self.rewind_enabled.save(fh)?;
        self.save_enabled.save(fh)?;
        // Ignore clear_save
        self.concurrent_dpad.save(fh)?;
        self.save_slot.save(fh)?;
        self.scale.save(fh)?;
        self.speed.save(fh)?;
        // Ignore genie_codes
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.pause_in_bg.load(fh)?;
        self.fullscreen.load(fh)?;
        self.vsync.load(fh)?;
        self.sound_enabled.load(fh)?;
        self.rewind_enabled.load(fh)?;
        self.save_enabled.load(fh)?;
        self.concurrent_dpad.load(fh)?;
        self.save_slot.load(fh)?;
        self.scale.load(fh)?;
        self.speed.load(fh)?;
        Ok(())
    }
}

impl Nes {
    pub(super) fn change_speed(&mut self, delta: f32) {
        if self.recording || self.playback {
            self.add_message("Speed changes disabled while recording or replaying");
        } else {
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
            self.cpu.bus.apu.set_speed(self.config.speed);
        }
    }

    pub(super) fn set_speed(&mut self, speed: f32) {
        if self.recording || self.playback {
            self.add_message("Speed changes disabled while recording or replaying");
        } else {
            self.config.speed = speed;
            self.cpu.bus.apu.set_speed(self.config.speed);
        }
    }

    pub(super) fn update_title(&mut self, s: &mut PixState) -> NesResult<()> {
        let mut title = String::new();
        if self.paused {
            title.push_str("Paused");
        } else {
            title.push_str(&format!("Save Slot: {}", self.config.save_slot));
            if self.config.speed != DEFAULT_SPEED {
                title.push_str(&format!(" - Speed: {:2.0}%", self.config.speed * 100.0));
            }
            if !self.config.sound_enabled {
                title.push_str(" - Muted");
            }
        }
        s.set_title(&title)?;
        Ok(())
    }
}

impl Default for NesConfig {
    fn default() -> Self {
        Self::new()
    }
}
