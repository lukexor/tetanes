use crate::{
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    ui::{debug::DEBUG_WIDTH, Ui},
    NesResult,
};
use pix_engine::{
    draw::Rect,
    pixel::{self, ColorType, Pixel},
    sprite::Sprite,
    StateData,
};

impl Ui {
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

        // TODO

        data.copy_draw_target(1, "menu")?;
        data.clear_draw_target();
        Ok(())
    }

    pub(super) fn create_textures(&mut self, data: &mut StateData) -> NesResult<()> {
        data.create_texture(
            1,
            "nes",
            ColorType::Rgb,
            Rect::new(0, 8, RENDER_WIDTH, RENDER_HEIGHT - 8), // Trims overscan
            Rect::new(0, 0, self.width, self.height),
        )?;
        data.create_texture(
            1,
            "message",
            ColorType::Rgba,
            Rect::new(0, 0, self.width, self.height),
            Rect::new(0, 0, self.width, self.height),
        )?;
        data.create_texture(
            1,
            "menu",
            ColorType::Rgba,
            Rect::new(0, 0, self.width, self.height),
            Rect::new(0, 0, self.width, self.height),
        )?;
        data.create_texture(
            1,
            "debug",
            ColorType::Rgba,
            Rect::new(0, 0, DEBUG_WIDTH, self.height),
            Rect::new(self.width, 0, DEBUG_WIDTH, self.height),
        )?;
        Ok(())
    }
}
