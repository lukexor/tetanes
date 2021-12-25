use super::{Mode, Nes, NesResult};
use crate::common::Powered;
use anyhow::Context;
use std::{fs::File, io::BufReader, path::Path};

pub(crate) fn is_nes_rom<P>(path: P) -> bool
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    path.extension().map(|ext| ext == "nes").unwrap_or(false)
}

impl Nes {
    /// Loads a ROM cartridge into memory
    pub(crate) fn load_rom(&mut self) -> NesResult<()> {
        self.mode = Mode::Paused;
        self.control_deck.power_off();
        let rom = File::open(&self.config.rom_path)
            .with_context(|| format!("failed to open rom {:?}", self.config.rom_path))?;
        let mut rom = BufReader::new(rom);
        self.control_deck
            .load_rom(&self.config.rom_path.to_string_lossy(), &mut rom)?;
        self.control_deck.power_on();
        self.mode = Mode::Playing;
        Ok(())
    }
}
