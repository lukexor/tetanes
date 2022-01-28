use crate::nes::{Mode, Nes};
use pix_engine::prelude::*;
use serde::{Deserialize, Serialize};

mod config;
mod help;
mod keybinds;
mod load_rom;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Menu {
    Help,
    Config,
    Keybind,
    LoadRom,
}

impl AsRef<str> for Menu {
    fn as_ref(&self) -> &str {
        match self {
            Self::Help => "Help",
            Self::Config => "Configuration",
            Self::Keybind => "Keybindings",
            Self::LoadRom => "Load ROM",
        }
    }
}

impl Nes {
    pub(crate) fn render_menu(&mut self, s: &mut PixState, mut menu: Menu) -> PixResult<()> {
        let mut bg = s.theme().colors.background;
        bg.set_alpha(200);
        s.fill(bg);
        s.rect([0, 0, s.width()? as i32, s.height()? as i32])?;
        s.no_stroke();
        s.fill(Color::WHITE);

        s.heading("Menu")?;
        if self.control_deck.is_running() && s.menu("< Exit")? {
            self.mode = Mode::Playing;
        }
        s.spacing()?;

        if s.tab_bar(
            "Menu",
            &[Menu::Help, Menu::Config, Menu::Keybind, Menu::LoadRom],
            &mut menu,
            |tab: &Menu, s: &mut PixState| match tab {
                Menu::Help => self.render_help(s),
                Menu::Config => self.render_config(s),
                Menu::Keybind => self.render_keybinds(s),
                Menu::LoadRom => self.render_load_rom(s),
            },
        )? {
            self.mode = Mode::InMenu(menu);
        }

        Ok(())
    }
}
