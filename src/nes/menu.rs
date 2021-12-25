use super::filesystem::is_nes_rom;
use crate::{
    apu::AudioChannel,
    nes::{Mode, Nes, WINDOW_HEIGHT, WINDOW_WIDTH},
    ppu::Filter,
};
use pix_engine::prelude::*;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, path::PathBuf};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Menu {
    Main,
    Help,
    Config,
    Keybind,
    LoadRom,
}

impl Nes {
    pub(crate) fn render_menu(&mut self, s: &mut PixState, menu: Menu) -> PixResult<()> {
        s.fill([0, 200]);
        s.rect([0, 0, s.width()? as i32, s.height()? as i32])?;
        s.no_stroke();
        s.fill(Color::WHITE);
        match menu {
            Menu::Main => self.render_main(s)?,
            Menu::Help => {
                s.heading("Help")?;
                if s.menu("< Menu")? {
                    self.mode = Mode::InMenu(Menu::Main);
                }
                s.spacing()?;
            }
            Menu::Config => self.render_config(s)?,
            Menu::Keybind => {
                s.heading("Keybindings")?;
                if s.menu("< Menu")? {
                    self.mode = Mode::InMenu(Menu::Main);
                }
                s.spacing()?;
            }
            Menu::LoadRom => self.render_load_rom(s)?,
        }
        Ok(())
    }

    fn render_main(&mut self, s: &mut PixState) -> PixResult<()> {
        s.heading("Menu")?;
        if s.menu("< Exit")? {
            self.mode = Mode::Playing;
        }
        s.spacing()?;
        if s.menu("Help")? {
            self.mode = Mode::InMenu(Menu::Help);
        }
        if s.menu("Configuration")? {
            self.mode = Mode::InMenu(Menu::Config);
        }
        if s.menu("Keybindings")? {
            self.mode = Mode::InMenu(Menu::Keybind);
        }
        if s.menu("Load ROM")? {
            self.mode = Mode::InMenu(Menu::LoadRom);
        }
        Ok(())
    }

    fn render_config(&mut self, s: &mut PixState) -> PixResult<()> {
        s.heading("Configuration")?;
        if s.menu("< Menu")? {
            self.mode = Mode::InMenu(Menu::Main);
        }
        s.spacing()?;

        s.collapsing_tree("General", |s: &mut PixState| {
            s.checkbox("Pause in Background", &mut self.config.pause_in_bg)?;

            let mut save_slot = self.config.save_slot as usize - 1;
            s.next_width(50);
            if s.select_box("Save Slot", &mut save_slot, &["1", "2", "3", "4"], 4)? {
                self.config.save_slot = save_slot as u8 + 1;
            }
            Ok(())
        })?;

        s.collapsing_tree("Emulation", |s: &mut PixState| {
            s.checkbox("Consistent Power-up RAM", &mut self.config.consistent_ram)?;
            s.checkbox("Concurrent D-Pad", &mut self.config.concurrent_dpad)?;

            s.next_width(s.theme().font_size * 15);
            if s.slider("Speed", &mut self.config.speed, 0.25, 2.0)? {
                self.set_speed(self.config.speed);
            }
            Ok(())
        })?;

        s.collapsing_tree("Sound", |s: &mut PixState| {
            s.checkbox("Enabled", &mut self.config.sound)?;
            s.spacing()?;

            s.text("Channels:")?;
            let mut pulse1 = self.control_deck.channel_enabled(AudioChannel::Pulse1);
            if s.checkbox("Pulse 1", &mut pulse1)? {
                self.control_deck.toggle_channel(AudioChannel::Pulse1);
            }
            let mut pulse2 = self.control_deck.channel_enabled(AudioChannel::Pulse2);
            if s.checkbox("Pulse 2", &mut pulse2)? {
                self.control_deck.toggle_channel(AudioChannel::Pulse2);
            }
            let mut triangle = self.control_deck.channel_enabled(AudioChannel::Triangle);
            if s.checkbox("Triangle", &mut triangle)? {
                self.control_deck.toggle_channel(AudioChannel::Triangle);
            }
            let mut noise = self.control_deck.channel_enabled(AudioChannel::Noise);
            if s.checkbox("Noise", &mut noise)? {
                self.control_deck.toggle_channel(AudioChannel::Noise);
            }
            let mut dmc = self.control_deck.channel_enabled(AudioChannel::Dmc);
            if s.checkbox("DMC", &mut dmc)? {
                self.control_deck.toggle_channel(AudioChannel::Dmc);
            }
            Ok(())
        })?;

        s.collapsing_tree("Video", |s: &mut PixState| {
            let mut scale = self.config.scale as usize - 1;
            s.next_width(50);
            if s.select_box("Scale", &mut scale, &["1", "2", "3", "4"], 4)? {
                self.config.scale = scale as f32 + 1.0;
                let width = (self.config.scale * WINDOW_WIDTH) as u32;
                let height = (self.config.scale * WINDOW_HEIGHT) as u32;
                s.set_window_dimensions((width, height))?;
                let (font_size, pad, ipady) = match scale {
                    0 => (6, 4, 3),
                    1 => (8, 6, 4),
                    2 => (12, 8, 6),
                    3 => (16, 10, 8),
                    _ => unreachable!("invalid scale"),
                };
                s.font_size(font_size)?;
                s.theme_mut().spacing.frame_pad = point!(pad, pad);
                s.theme_mut().spacing.item_pad = point!(pad, ipady);
            }

            let mut enabled = self.control_deck.filter() == Filter::Ntsc;
            if s.checkbox("NTSC Filter", &mut enabled)? {
                self.control_deck
                    .set_filter(if enabled { Filter::Ntsc } else { Filter::None });
            }

            if s.checkbox("Fullscreen", &mut self.config.fullscreen)? {
                s.fullscreen(self.config.fullscreen)?;
            }

            if s.checkbox("VSync Enabled", &mut self.config.vsync)? {
                s.vsync(self.config.vsync)?;
            }
            Ok(())
        })?;

        Ok(())
    }

    fn render_load_rom(&mut self, s: &mut PixState) -> PixResult<()> {
        s.heading("Load ROM")?;
        if s.menu("< Menu")? {
            self.mode = Mode::InMenu(Menu::Main);
        }
        s.spacing()?;

        if self.paths.is_empty() {
            self.update_paths()?;
        }

        if let Some(error) = &self.error {
            s.fill(s.theme().colors.error);
            s.text(&error)?;
        } else {
            let line_height = s.theme().font_size as i32 + 4 * s.theme().spacing.item_pad.y();
            let displayed_count =
                (s.height()? as usize - s.cursor_pos().y() as usize) / line_height as usize;
            let rom_dir = if self.config.rom_path.is_file() {
                self.config.rom_path.parent().unwrap()
            } else {
                self.config.rom_path.as_path()
            };
            let path_list: Vec<Cow<'_, str>> = self
                .paths
                .iter()
                .map(|p| p.strip_prefix(&rom_dir).unwrap_or(p).to_string_lossy())
                .collect();
            s.next_width(s.width()? - 2 * s.theme().spacing.frame_pad.x() as u32 - 12);
            s.select_list(
                rom_dir.to_string_lossy(),
                &mut self.selected_path,
                &path_list,
                displayed_count,
            )?;
            if s.button("Open")? {
                let parent = self.config.rom_path.parent();
                if self.selected_path == 0 && parent.is_some() {
                    if let Some(path) = parent {
                        self.config.rom_path = path.to_path_buf();
                        self.update_paths()?;
                    }
                } else {
                    self.config.rom_path = self.paths[self.selected_path].to_path_buf();
                    if self.config.rom_path.is_dir() {
                        self.update_paths()?;
                    } else if is_nes_rom(&self.config.rom_path) {
                        self.load_rom()?;
                        s.resume_audio();
                    }
                }
                self.selected_path = 0;
            }
        }
        Ok(())
    }

    fn update_paths(&mut self) -> PixResult<()> {
        self.paths.clear();
        let mut path = self.config.rom_path.as_path();
        if path.is_file() {
            path = path.parent().expect("file should have a parent folder");
        }
        match path.read_dir() {
            Ok(read_dir) => {
                read_dir
                    .filter_map(Result::ok)
                    .map(|f| f.path())
                    .filter(|p| p.is_dir() || is_nes_rom(p))
                    .for_each(|p| self.paths.push(p));
                self.paths.sort();
                if path.parent().is_some() {
                    self.paths.insert(0, PathBuf::from("../"));
                }
            }
            Err(err) => {
                self.error = Some(err.to_string());
            }
        }
        Ok(())
    }
}
