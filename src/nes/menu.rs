use crate::nes::{Mode, Nes};
use keybinds::Player;
use pix_engine::prelude::*;
use serde::{Deserialize, Serialize};

pub(crate) mod about;
pub(crate) mod config;
pub(crate) mod keybinds;
pub(crate) mod load_rom;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum Menu {
    Config,
    Keybind,
    LoadRom,
    About,
}

impl AsRef<str> for Menu {
    fn as_ref(&self) -> &str {
        match self {
            Self::Config => "Configuration",
            Self::Keybind => "Keybindings",
            Self::LoadRom => "Load ROM",
            Self::About => "About",
        }
    }
}

impl Nes {
    pub(crate) fn render_menu(
        &mut self,
        s: &mut PixState,
        menu: Menu,
        player: Player,
    ) -> PixResult<()> {
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

        let render_menu = |tab: &Menu, s: &mut PixState| match tab {
            Menu::Config => self.render_config(s),
            Menu::Keybind => self.render_keybinds(s, menu, player),
            Menu::LoadRom => self.render_load_rom(s),
            Menu::About => self.render_about(s),
        };
        let mut menu_selection = menu;
        if s.tab_bar(
            "Menu",
            &[Menu::Config, Menu::Keybind, Menu::LoadRom, Menu::About],
            &mut menu_selection,
            render_menu,
        )? {
            self.mode = Mode::InMenu(menu_selection, player);
        }

        Ok(())
    }
}
