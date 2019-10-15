use crate::{ui::Ui, NesResult};
use pix_engine::{
    pixel::{self, Pixel},
    sprite::Sprite,
    StateData,
};

impl Ui {
    pub(super) fn draw_menu(&mut self, data: &mut StateData) -> NesResult<()> {
        // Darken background
        data.set_draw_target(Sprite::new(self.width, self.height));
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
        Ok(())
    }
}
