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
        data.draw_rect(
            50,
            50,
            self.width - 100,
            self.height - 100,
            pixel::VERY_DARK_GRAY,
        );
        data.draw_rect(
            53,
            53,
            self.width - 106,
            self.height - 106,
            pixel::DARK_GRAY,
        );

        // TODO

        data.copy_draw_target(1, "menu")?;
        Ok(())
    }
}
