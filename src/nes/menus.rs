use crate::{
    nes::{debug::DEBUG_WIDTH, Nes},
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    NesResult,
};
use pix_engine::{
    draw::Rect,
    pixel::{self, ColorType, Pixel},
    sprite::Sprite,
    StateData,
};

pub(super) const MSG_HEIGHT: u32 = 25 * 4 + 5; // 5 lines worth of messages

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
    pub(super) fn paused(&mut self, paused: bool) {
        if !self.paused && paused {
            self.set_static_message("Paused");
        } else if !paused {
            self.unset_static_message("Paused");
        }
        self.paused = paused;
    }

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
            let mut msg_box = Sprite::new(self.width, MSG_HEIGHT);
            data.set_draw_target(&mut msg_box);
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
            data.copy_draw_target(self.nes_window, "message")?;
            data.clear_draw_target();
        }
        Ok(())
    }

    pub(super) fn draw_menu(&mut self, data: &mut StateData) -> NesResult<()> {
        // Darken background
        let mut menu = Sprite::new(self.width, self.height);
        data.set_draw_target(&mut menu);
        data.fill(Pixel([0, 0, 0, 128]));
        let (mut x, mut y) = (50, 50);
        data.fill_rect(
            x,
            y,
            self.width - 100,
            self.height - 100,
            pixel::VERY_DARK_GRAY,
        );
        x += 3;
        y += 3;
        data.fill_rect(x, y, self.width - 106, self.height - 106, pixel::DARK_GRAY);
        x += 10;
        y += 10;
        data.set_draw_scale(3);
        data.draw_string(x, y, "Configuration", pixel::WHITE);
        y += 50;
        data.set_draw_scale(2);
        data.draw_string(x, y, "Not yet implemented", pixel::WHITE);

        // TODO draw menu config, add interactivity

        data.copy_draw_target(self.nes_window, "menu")?;
        data.clear_draw_target();
        Ok(())
    }

    pub(super) fn create_textures(&mut self, data: &mut StateData) -> NesResult<()> {
        data.create_texture(
            self.nes_window,
            "nes",
            ColorType::Rgb,
            Rect::new(0, 8, RENDER_WIDTH, RENDER_HEIGHT - 8), // Trims overscan
            Rect::new(0, 0, self.width, self.height),
        )?;
        data.create_texture(
            self.nes_window,
            "message",
            ColorType::Rgba,
            Rect::new(0, 0, self.width, MSG_HEIGHT),
            Rect::new(0, 0, self.width, MSG_HEIGHT),
        )?;
        data.create_texture(
            self.nes_window,
            "menu",
            ColorType::Rgba,
            Rect::new(0, 0, self.width, self.height),
            Rect::new(0, 0, self.width, self.height),
        )?;
        data.create_texture(
            self.nes_window,
            "debug",
            ColorType::Rgba,
            Rect::new(0, 0, DEBUG_WIDTH, self.height),
            Rect::new(self.width, 0, DEBUG_WIDTH, self.height),
        )?;
        Ok(())
    }

    pub(super) fn check_focus(&mut self) {
        let id = self.focused_window;
        if id != self.nes_window
            && Some(id) != self.ppu_viewer_window
            && Some(id) != self.nt_viewer_window
        {
            // Only pause and set lost_focus if we weren't already paused
            if !self.paused {
                self.lost_focus = true;
            }
            self.paused(true);
        } else if self.lost_focus {
            self.lost_focus = false;
            // Only unpause if we weren't paused as a result of losing focus
            self.paused(false);
        }
    }
}
