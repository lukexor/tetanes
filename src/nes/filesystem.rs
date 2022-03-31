use super::{menu::Player, Menu, Mode, Nes, NesResult};
use anyhow::{anyhow, Context};
use flate2::{bufread::DeflateDecoder, write::DeflateEncoder, Compression};
use pix_engine::prelude::PixState;
use std::{
    ffi::OsStr,
    fs::{create_dir_all, File},
    io::{BufReader, BufWriter, Read, Write},
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
    f.write_all(&SAVE_FILE_MAGIC)?;
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

pub(crate) fn encode_data(data: &[u8]) -> NesResult<Vec<u8>> {
    let mut encoded = vec![];
    let mut encoder = DeflateEncoder::new(&mut encoded, Compression::default());
    encoder
        .write_all(data)
        .with_context(|| anyhow!("failed to encode data"))?;
    encoder
        .finish()
        .with_context(|| anyhow!("failed to write data"))?;
    Ok(encoded)
}

pub(crate) fn decode_data(data: &[u8]) -> NesResult<Vec<u8>> {
    let mut decoded = vec![];
    let mut decoder = DeflateDecoder::new(BufReader::new(data));
    decoder
        .read_to_end(&mut decoded)
        .with_context(|| anyhow!("failed to read data"))?;
    Ok(decoded)
}

pub(crate) fn save_data<P>(path: P, data: &[u8]) -> NesResult<()>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let directory = path.parent().expect("can not save to root path");
    if !directory.exists() {
        create_dir_all(directory)
            .with_context(|| anyhow!("failed to create directory {:?}", directory.display()))?;
    }

    let write_data = || {
        let mut writer = BufWriter::new(
            File::create(&path)
                .with_context(|| anyhow!("failed to create file {:?}", path.display()))?,
        );
        write_save_header(&mut writer)
            .with_context(|| anyhow!("failed to write header {:?}", path.display()))?;
        let mut encoder = DeflateEncoder::new(writer, Compression::default());
        encoder
            .write_all(data)
            .with_context(|| anyhow!("failed to encode file {:?}", path.display()))?;
        encoder
            .finish()
            .with_context(|| anyhow!("failed to write file {:?}", path.display()))?;
        Ok(())
    };

    if path.exists() {
        // Check if exists and header is different, so we avoid overwriting
        let mut reader = BufReader::new(
            File::open(&path)
                .with_context(|| anyhow!("failed to open file {:?}", path.display()))?,
        );
        validate_save_header(&mut reader)
            .with_context(|| anyhow!("failed to validate header {:?}", path.display()))
            .and_then(|_| write_data())?;
    } else {
        write_data()?;
    }
    Ok(())
}

pub(crate) fn load_data<P>(path: P) -> NesResult<Vec<u8>>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let mut reader = BufReader::new(
        File::open(&path).with_context(|| anyhow!("Failed to open file {:?}", path.display()))?,
    );
    let mut bytes = vec![];
    // Don't care about the size read
    let _ = validate_save_header(&mut reader)
        .with_context(|| anyhow!("failed to validate header {:?}", path.display()))
        .and_then(|_| {
            let mut decoder = DeflateDecoder::new(reader);
            decoder
                .read_to_end(&mut bytes)
                .with_context(|| anyhow!("failed to read file {:?}", path.display()))
        })?;
    Ok(bytes)
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
    pub(crate) fn load_rom(&mut self, s: &mut PixState) {
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
                return;
            }
        };
        let name = self
            .config
            .rom_path
            .file_name()
            .map_or_else(|| "unknown".into(), OsStr::to_string_lossy);
        let mut rom = BufReader::new(rom);
        match self.control_deck.load_rom(&name, &mut rom) {
            Ok(()) => {
                s.resume_audio();
                if let Err(err) = self.load_sram() {
                    log::error!("{:?}", err);
                    self.add_message("Failed to load game state");
                }
                self.mode = Mode::Playing;
            }
            Err(err) => {
                self.mode = Mode::InMenu(Menu::LoadRom, Player::One);
                self.error = Some(err.to_string());
            }
        }
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
