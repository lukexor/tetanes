use crate::{nes::Nes, NesResult};
use pix_engine::prelude::*;

impl Nes {
    pub fn draw_config_menu(&mut self, s: &mut PixState) -> NesResult<()> {
        // Darken background
        s.background(rgb!(0, 128));
        s.clear();
        let mut p = point!(50, 50);
        let w = self.width - 100;
        let h = self.height - 100;

        s.fill(DARK_GRAY);
        s.rect((p, w, h))?;
        p += 3;
        s.rect((p, w - 6, h - 6))?;
        p += 10;
        s.fill(WHITE);
        s.text_size(36);
        s.text(p, "Configuration")?;
        p.y += 50;
        s.text_size(24);
        s.text(p, "Not yet implemented")?;

        // TODO draw menu config, add interactivity

        Ok(())
    }
}
