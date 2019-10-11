use crate::ui::Ui;
use pix_engine::{pixel, Rgba, Sprite, StateData};
use std::{env, path::PathBuf};

pub(super) const DEFAULT_SPEED: f64 = 1.0; // 100% - 60 Hz
const MIN_SPEED: f64 = 0.25; // 25% - 240 Hz
const MAX_SPEED: f64 = 2.0; // 200% - 30 Hz

pub struct UiSettings {
    pub path: PathBuf,
    pub debug: bool,
    pub fullscreen: bool,
    pub vsync: bool,
    pub sound_enabled: bool,
    pub save_enabled: bool,
    pub concurrent_dpad: bool,
    pub randomize_ram: bool,
    pub save_slot: u8,
    pub scale: u32,
    pub speed: f64,
    pub genie_codes: Vec<String>,
}

impl UiSettings {
    pub fn new() -> Self {
        Self {
            path: env::current_dir().unwrap_or_default(),
            debug: false,
            fullscreen: false,
            vsync: false,
            sound_enabled: true,
            save_enabled: true,
            concurrent_dpad: false,
            randomize_ram: false,
            save_slot: 1,
            scale: 3,
            speed: 1.0,
            genie_codes: Vec::new(),
        }
    }
}

impl Ui {
    pub(super) fn change_speed(&mut self, delta: f64) {
        self.settings.speed += DEFAULT_SPEED * delta;
        if self.settings.speed < MIN_SPEED {
            self.settings.speed = MIN_SPEED;
        } else if self.settings.speed > MAX_SPEED {
            self.settings.speed = MAX_SPEED;
        }
        self.console.set_speed(self.settings.speed);
    }

    pub(super) fn update_title(&mut self, data: &mut StateData) {
        let mut title = String::new();
        if self.paused {
            title.push_str("Paused");
        } else {
            title.push_str(&format!("Save Slot: {}", self.settings.save_slot));
            if self.settings.speed != DEFAULT_SPEED {
                title.push_str(&format!(" - Speed: {}%", self.settings.speed * 100.0));
            }
        }
        data.set_title(&title);
    }

    pub(super) fn draw_menu(&mut self, data: &mut StateData) {
        // Darken background
        data.set_draw_target(Sprite::new_rgba8(self.width, self.height));
        data.fill(Rgba([0, 0, 0, 128]));
        data.fill_rect(
            50,
            50,
            self.width - 100,
            self.height - 100,
            pixel::VERY_DARK_GRAY,
        );
        data.fill_rect(
            53,
            53,
            self.width - 106,
            self.height - 106,
            pixel::DARK_GRAY,
        );

        // TODO

        let pixels = &data.get_draw_target().raw_pixels();
        data.copy_texture("menu", pixels);
    }
}
