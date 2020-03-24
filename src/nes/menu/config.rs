use crate::{nes::Nes, NesResult};
use pix_engine::{
    pixel::{self, Pixel},
    sprite::Sprite,
    StateData,
};

impl Nes {
    pub fn draw_config_menu(&mut self, data: &mut StateData) -> NesResult<()> {
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
}
