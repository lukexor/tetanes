use crate::{
    nes::{debug::DEBUG_WIDTH, Nes},
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    NesResult,
};
use pix_engine::{
    draw::Rect,
    image::Image,
    pixel::{self, ColorType, Pixel},
    StateData,
};

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

    pub(super) fn draw(&mut self, data: &mut StateData) -> NesResult<()> {
        match &self.menu_type {
            MenuType::OpenRom => open_rom::draw_open_menu(data),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Message {
    timer: f32,
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

    pub(super) fn draw_messages(&mut self, elapsed: f32, data: &mut StateData) -> NesResult<()> {
        self.messages.retain(|msg| !msg.timed || msg.timer > 0.0);
        self.messages.dedup();
        if !self.messages.is_empty() {
            let msg_box = Image::new_ref(self.width, MSG_HEIGHT);
            data.set_draw_target(msg_box);
            let mut y = 5;
            data.set_draw_scale(2);
            for msg in self.messages.iter_mut() {
                msg.timer -= elapsed;
                data.fill_rect(0, y - 5, self.width, 25, Pixel([0, 0, 0, 200]));
                let mut x = 10;
                for s in msg.text.split_whitespace() {
                    let curr_width = s.len() as u32 * 16;
                    if x + curr_width >= self.width {
                        x = 10;
                        y += 20;
                        data.draw_string(x, y, s, pixel::RED);
                    } else {
                        data.draw_string(x, y, s, pixel::RED);
                    }
                    x += curr_width;
                    data.draw_string(x, y, " ", pixel::RED);
                    x += 16;
                }
                y += 20;
            }
            data.set_draw_scale(1);
            data.copy_draw_target("message")?;
        }
        Ok(())
    }

    pub(super) fn create_textures(&mut self, data: &mut StateData) -> NesResult<()> {
        data.create_texture(
            "nes",
            ColorType::Rgba,
            Rect::new(0, 8, RENDER_WIDTH, RENDER_HEIGHT - 8), // Trims overscan
            Rect::new(0, 0, self.width, self.height),
        )?;
        data.create_texture(
            "message",
            ColorType::Rgba,
            Rect::new(0, 0, self.width, MSG_HEIGHT),
            Rect::new(0, 0, self.width, MSG_HEIGHT),
        )?;
        data.create_texture(
            "menu",
            ColorType::Rgba,
            Rect::new(0, 0, self.width, self.height),
            Rect::new(0, 0, self.width, self.height),
        )?;
        data.create_texture(
            "debug",
            ColorType::Rgba,
            Rect::new(0, 0, DEBUG_WIDTH, self.height),
            Rect::new(self.width, 0, DEBUG_WIDTH, self.height),
        )?;
        Ok(())
    }
}
