use crate::{
    common::{config_path, SAVE_DIR, SRAM_DIR},
    nes::{config::KEYBINDS, Nes, SETTINGS},
};
use pix_engine::prelude::*;

impl Nes {
    pub(super) fn render_about(&mut self, s: &mut PixState) -> PixResult<()> {
        s.heading("TetaNES v0.8.0")?;
        s.spacing()?;

        if s.link("github.com/lukexor/tetanes")? {
            s.open_url("https://github.com/lukexor/tetanes")?;
        }
        s.spacing()?;

        s.text("Configuration:")?;

        s.bullet("Keybinds: ")?;
        s.same_line(None);
        s.monospace(config_path(KEYBINDS).to_string_lossy())?;

        s.bullet("Settings: ")?;
        s.same_line(None);
        s.monospace(config_path(SETTINGS).to_string_lossy())?;

        s.text("Directories:")?;

        s.bullet("Save states: ")?;
        s.same_line(None);
        s.monospace(config_path(SAVE_DIR).to_string_lossy())?;

        s.bullet("Battery-Backed Save RAM: ")?;
        s.same_line(None);
        s.monospace(config_path(SRAM_DIR).to_string_lossy())?;

        Ok(())
    }
}
