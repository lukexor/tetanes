use super::{menu::Player, Menu, Mode, Nes, NesResult};
use anyhow::Context;
use pix_engine::prelude::PixState;
use std::{fs::File, io::BufReader, path::Path};

pub(crate) fn is_nes_rom<P>(path: P) -> bool
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    // FIXME: Check nes header instead
    path.extension().map_or(false, |ext| ext == "nes")
}

pub(crate) fn is_playback_file<P>(path: P) -> bool
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    // FIXME: Also check playback header
    path.extension().map_or(false, |ext| ext == "playback")
}

impl Nes {
    /// Loads a ROM cartridge into memory
    pub(crate) fn load_rom(&mut self, s: &mut PixState) -> NesResult<()> {
        self.mode = Mode::Paused;
        s.pause_audio();
        let rom = match File::open(&self.config.rom_path)
            .with_context(|| format!("failed to open rom {:?}", self.config.rom_path))
        {
            Ok(rom) => rom,
            Err(err) => {
                self.mode = Mode::InMenu(Menu::LoadRom, Player::One);
                self.error = Some(err.to_string());
                return Ok(());
            }
        };
        let mut rom = BufReader::new(rom);
        match self
            .control_deck
            .load_rom(&self.config.rom_path.to_string_lossy(), &mut rom)
        {
            Ok(()) => {
                s.resume_audio();
                self.mode = Mode::Playing;
            }
            Err(err) => {
                self.mode = Mode::InMenu(Menu::LoadRom, Player::One);
                self.error = Some(err.to_string());
            }
        }
        Ok(())
    }
}
