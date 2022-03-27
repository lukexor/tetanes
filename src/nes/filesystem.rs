use super::{menu::Player, Menu, Mode, Nes, NesResult};
use anyhow::{anyhow, Context};
use pix_engine::prelude::PixState;
use std::{
    fs::File,
    io::{BufReader, Read, Write},
    path::Path,
};

const SAVE_FILE_MAGIC_LEN: usize = 8;
const SAVE_FILE_MAGIC: [u8; SAVE_FILE_MAGIC_LEN] = *b"TETANES\x1a";
// MAJOR version of SemVer. Increases when save file format isn't backwards compatible
const VERSION: u8 = 0;

/// Writes a header including a magic string and a version
///
/// # Errors
///
/// If the header fails to write to disk, then an error is returned.
pub(crate) fn write_save_header<F: Write>(f: &mut F) -> NesResult<()> {
    f.write_all(&bincode::serialize(&SAVE_FILE_MAGIC)?)?;
    f.write_all(&[VERSION])?;
    Ok(())
}

/// Verifies a `TetaNES` saved state header.
///
/// # Errors
///
/// If the header fails to validate, then an error is returned.
pub(crate) fn validate_save_header<F: Read>(f: &mut F) -> NesResult<()> {
    let mut magic = [0u8; SAVE_FILE_MAGIC_LEN];
    f.read_exact(&mut magic)?;
    if magic == SAVE_FILE_MAGIC {
        let mut version = [0u8];
        f.read_exact(&mut version)?;
        if version[0] == VERSION {
            Ok(())
        } else {
            Err(anyhow!(
                "invalid save file version. current: {}, save file: {}",
                VERSION,
                version[0],
            ))
        }
    } else {
        Err(anyhow!("invalid save file format"))
    }
}

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
        self.error = None;
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
        let name = self
            .config
            .rom_path
            .file_name()
            .map(|f| f.to_string_lossy())
            .unwrap_or_else(|| "Unknown".into());
        let mut rom = BufReader::new(rom);
        match self.control_deck.load_rom(&name, &mut rom) {
            Ok(()) => {
                s.resume_audio();
                self.load_sram()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_header() {
        let mut file = Vec::new();
        assert!(write_save_header(&mut file).is_ok(), "write save header");
        assert!(
            validate_save_header(&mut file.as_slice()).is_ok(),
            "validate save header"
        );
    }
}
