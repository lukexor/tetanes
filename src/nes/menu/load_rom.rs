use crate::nes::{is_nes_rom, Nes};
use pix_engine::prelude::*;
use std::{borrow::Cow, path::PathBuf};

impl Nes {
    pub(super) fn render_load_rom(&mut self, s: &mut PixState) -> PixResult<()> {
        let colors = s.theme().colors;
        let font_size = s.theme().font_size;
        let spacing = s.theme().spacing;

        if self.paths.is_empty() {
            self.update_paths()?;
        }

        if let Some(error) = &self.error {
            s.fill(colors.error);
            s.text(&error)?;
        } else {
            let line_height = font_size as i32 + 4 * spacing.item_pad.y();
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

            s.fill(colors.secondary);
            s.next_width((s.ui_width()? - spacing.scroll_size) as u32);
            s.select_list(
                format!("{}:", rom_dir.to_string_lossy()),
                &mut self.selected_path,
                &path_list,
                displayed_count,
            )?;
            let path = self.paths[self.selected_path].clone();
            if s.dbl_clicked() {
                if self.selected_path == 0 {
                    if let Some(parent) = self.config.rom_path.parent() {
                        self.config.rom_path = parent.to_path_buf();
                        self.update_paths()?;
                    }
                } else if path.is_dir() {
                    self.config.rom_path = path.clone();
                    self.update_paths()?;
                }
            }
            if !is_nes_rom(&path) {
                s.disable();
            }
            if s.dbl_clicked() || s.button("Open")? {
                self.config.rom_path = path;
                self.selected_path = 0;
                self.load_rom(s)?;
            }
            s.no_disable();
        }
        Ok(())
    }

    fn update_paths(&mut self) -> PixResult<()> {
        self.selected_path = 0;
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
