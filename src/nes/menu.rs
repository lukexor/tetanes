use crate::{
    nes::{debug::DEBUG_WIDTH, Nes},
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    NesResult,
};
use pix_engine::prelude::*;

mod config;
mod help;
mod keybinds;
mod open_rom;

pub(super) const MSG_HEIGHT: u32 = 25 * 4 + 5; // 5 lines worth of messages

#[derive(Clone)]
pub(super) struct Menu {
    menu_type: MenuType,
    width: u32,
    height: u32,
    image: Image,
    open: bool,
    // keybinds: Vec<PixEvent>, // TODO
}

#[derive(Clone, PartialEq)]
pub(super) enum MenuType {
    Config,
    Help,
    Keybind,
    OpenRom,
}

impl Menu {
    pub(super) fn new(menu_type: MenuType, width: u32, height: u32) -> Self {
        Self {
            menu_type,
            width,
            height,
            image: Image::new(width, height),
            open: false,
        }
    }

    pub(super) fn _title(&self) -> &'static str {
        match &self.menu_type {
            MenuType::Config => "Configuration",
            MenuType::Help => "Help",
            MenuType::Keybind => "Keybindings",
            MenuType::OpenRom => "Open Rom",
        }
    }

    pub(super) fn draw(&mut self, s: &mut PixState) -> NesResult<()> {
        match &self.menu_type {
            MenuType::OpenRom => open_rom::draw_open_menu(s),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Message {
    timer: f64,
    timed: bool,
    text: String,
}

impl Message {
    pub(super) fn new(text: &str) -> Self {
        Self {
            timer: 5.0,
            timed: true,
            text: text.to_string(),
        }
    }
    pub(super) fn new_static(text: &str) -> Self {
        Self {
            timer: 0.0,
            timed: false,
            text: text.to_string(),
        }
    }
}

impl Nes {
    pub(super) fn add_message(&mut self, text: &str) {
        self.messages.push(Message::new(text));
    }

    pub(super) fn set_static_message(&mut self, text: &str) {
        self.messages.push(Message::new_static(text));
    }

    pub(super) fn unset_static_message(&mut self, text: &str) {
        self.messages.retain(|msg| msg.text != text);
    }

    pub(super) fn draw_messages(&mut self, s: &mut PixState) -> NesResult<()> {
        self.messages.retain(|msg| !msg.timed || msg.timer > 0.0);
        self.messages.dedup();
        if !self.messages.is_empty() {
            let mut p = point!(0, 5);
            s.text_size(24);
            for msg in self.messages.iter_mut() {
                msg.timer -= s.delta_time();
                s.fill(rgb!(0, 200));
                s.rect((0, p.y - 5, self.width, 25))?;
                p.x = 10;
                s.fill(RED);
                for msg in msg.text.split_whitespace() {
                    let curr_width = msg.len() as i32 * 16;
                    if p.x + curr_width >= self.width as i32 {
                        p.x = 10;
                        p.y += 20;
                        s.text(p, msg)?;
                    } else {
                        s.text(p, msg)?;
                    }
                    p.x += curr_width;
                    s.text(p, " ")?;
                    p.x += 16;
                }
                p.y += 20;
            }
        }
        Ok(())
    }

    pub(super) fn create_textures(&mut self, _s: &mut PixState) -> NesResult<()> {
        // s.create_texture(
        //     "nes",
        //     ColorType::Rgba,
        //     rect!(0, 8, RENDER_WIDTH, RENDER_HEIGHT - 8), // Trims overscan
        //     rect!(0, 0, self.width, self.height),
        // )?;
        // s.create_texture(
        //     "message",
        //     ColorType::Rgba,
        //     rect!(0, 0, self.width, MSG_HEIGHT),
        //     rect!(0, 0, self.width, MSG_HEIGHT),
        // )?;
        // s.create_texture(
        //     "menu",
        //     ColorType::Rgba,
        //     rect!(0, 0, self.width, self.height),
        //     rect!(0, 0, self.width, self.height),
        // )?;
        // s.create_texture(
        //     "debug",
        //     ColorType::Rgba,
        //     rect!(0, 0, DEBUG_WIDTH, self.height),
        //     rect!(self.width, 0, DEBUG_WIDTH, self.height),
        // )?;
        Ok(())
    }
}
