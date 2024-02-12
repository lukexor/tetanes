use super::{Nes, NesResult};
use crate::{
    common::Regional,
    nes::{config::Config, menu::Menu, state::Mode},
};
use anyhow::Context;
use flate2::{bufread::DeflateDecoder, write::DeflateEncoder, Compression};
use std::{
    ffi::OsStr,
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
};

const SAVE_FILE_MAGIC_LEN: usize = 8;
const SAVE_FILE_MAGIC: [u8; SAVE_FILE_MAGIC_LEN] = *b"TETANES\x1a";
const MAJOR_VERSION: &str = env!("CARGO_PKG_VERSION_MAJOR");

/// Writes a header including a magic string and a version
///
/// # Errors
///
/// If the header fails to write to disk, then an error is returned.
pub(crate) fn write_save_header<F: Write>(f: &mut F) -> NesResult<()> {
    f.write_all(&SAVE_FILE_MAGIC)?;
    f.write_all(MAJOR_VERSION.as_bytes())?;
    Ok(())
}

/// Verifies a `TetaNES` saved state header.
///
/// # Errors
///
/// If the header fails to validate, then an error is returned.
pub(crate) fn validate_save_header<F: Read>(f: &mut F) -> NesResult<()> {
    use anyhow::anyhow;

    let mut magic = [0u8; SAVE_FILE_MAGIC_LEN];
    f.read_exact(&mut magic)?;
    if magic == SAVE_FILE_MAGIC {
        let mut version = [0u8];
        f.read_exact(&mut version)?;
        if version == MAJOR_VERSION.as_bytes() {
            Ok(())
        } else {
            Err(anyhow!(
                "invalid save file version. current: {}, save file: {}",
                MAJOR_VERSION,
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
    encoder.write_all(data).context("failed to encode data")?;
    encoder.finish().context("failed to write data")?;
    Ok(encoded)
}

pub(crate) fn decode_data(data: &[u8]) -> NesResult<Vec<u8>> {
    let mut decoded = vec![];
    let mut decoder = DeflateDecoder::new(BufReader::new(data));
    decoder
        .read_to_end(&mut decoded)
        .context("failed to read data")?;
    Ok(decoded)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn save_data<P>(_path: P, _data: &[u8]) -> NesResult<()>
where
    P: AsRef<Path>,
{
    // TODO: provide file download?
    anyhow::bail!("not implemented")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_data<P>(path: P, data: &[u8]) -> NesResult<()>
where
    P: AsRef<Path>,
{
    use std::io::BufWriter;

    let path = path.as_ref();
    let directory = path.parent().expect("can not save to root path");
    if !directory.exists() {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("failed to create directory {directory:?}"))?;
    }

    let write_data = || {
        let mut writer = BufWriter::new(
            std::fs::File::create(path)
                .with_context(|| format!("failed to create file {path:?}"))?,
        );
        write_save_header(&mut writer)
            .with_context(|| format!("failed to write header {path:?}"))?;
        let mut encoder = DeflateEncoder::new(writer, Compression::default());
        encoder
            .write_all(data)
            .with_context(|| format!("failed to encode file {path:?}"))?;
        encoder
            .finish()
            .with_context(|| format!("failed to write file {path:?}"))?;
        Ok(())
    };

    if path.exists() {
        // Check if exists and header is different, so we avoid overwriting
        let mut reader = BufReader::new(
            std::fs::File::open(path).with_context(|| format!("failed to open file {path:?}"))?,
        );
        validate_save_header(&mut reader)
            .with_context(|| format!("failed to validate header {path:?}"))
            .and_then(|_| write_data())?;
    } else {
        write_data()?;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn load_data<P>(_path: P) -> NesResult<Vec<u8>>
where
    P: AsRef<Path>,
{
    // TODO: provide file upload?
    anyhow::bail!("not implemented")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load_data<P>(path: P) -> NesResult<Vec<u8>>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let mut reader = BufReader::new(
        std::fs::File::open(path).with_context(|| format!("Failed to open file {path:?}"))?,
    );
    let mut bytes = vec![];
    // Don't care about the size read
    let _ = validate_save_header(&mut reader)
        .with_context(|| format!("failed to validate header {path:?}"))
        .and_then(|_| {
            let mut decoder = DeflateDecoder::new(reader);
            decoder
                .read_to_end(&mut bytes)
                .with_context(|| format!("failed to read file {path:?}"))
        })?;
    Ok(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
#[inline]
pub(crate) fn filename(path: &Path) -> &str {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_else(|| {
            log::warn!("invalid rom_path: {path:?}");
            "??"
        })
}

impl Nes {
    /// Loads a ROM cartridge into memory from a path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_rom_path(&mut self, path: impl AsRef<std::path::Path>) {
        let path = path.as_ref();
        let filename = filename(path);
        match std::fs::File::open(path).with_context(|| format!("failed to open rom {path:?}")) {
            Ok(mut rom) => self.load_rom(filename, &mut rom),
            Err(err) => {
                log::error!("{path:?}: {err:?}");
                self.mode = Mode::Menu(Menu::LoadRom);
                self.error = Some(format!("Failed to open ROM {filename:?}"));
            }
        }
    }

    /// Loads a ROM cartridge into memory from a reader.
    pub fn load_rom(&mut self, filename: &str, rom: &mut impl Read) {
        self.pause_play();
        match self.control_deck.load_rom(filename, rom) {
            Ok(()) => {
                self.error = None;
                self.window.set_title(&filename.replace(".nes", ""));
                if let Err(err) = self.mixer.play() {
                    self.add_message(format!("failed to start audio: {err:?}"));
                }
                self.config.region = self.control_deck.region();
                if let Err(err) = self.load_sram() {
                    log::error!("{:?}: {:?}", self.config.rom_path, err);
                    self.add_message("Failed to load game state");
                }
                self.resume_play();
            }
            Err(err) => {
                log::error!("{:?}, {:?}", self.config.rom_path, err);
                self.mode = Mode::Menu(Menu::LoadRom);
                self.error = Some(format!("Failed to load ROM {filename:?}"));
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(path) = self.save_path(self.config.save_slot) {
                if self.config.load_on_start && path.exists() {
                    self.load_state(self.config.save_slot);
                }
            }
            self.load_replay();
        }
    }

    /// Returns the path where battery-backed Save RAM files are stored if a ROM is loaded. Returns
    /// `None` if no ROM is loaded.
    pub fn sram_path(&self) -> Option<PathBuf> {
        self.control_deck.loaded_rom().as_ref().and_then(|rom| {
            PathBuf::from(rom)
                .file_stem()
                .and_then(OsStr::to_str)
                .map(|save_name| {
                    Config::directory()
                        .join("sram")
                        .join(save_name)
                        .with_extension("sram")
                })
        })
    }

    /// Returns the path where Save states are stored if a ROM is loaded. Returns `None` if no ROM
    /// is loaded.
    pub fn save_path(&self, slot: u8) -> Option<PathBuf> {
        self.control_deck.loaded_rom().as_ref().and_then(|rom| {
            PathBuf::from(rom)
                .file_stem()
                .and_then(OsStr::to_str)
                .map(|save_name| {
                    Config::directory()
                        .join("save")
                        .join(save_name)
                        .join(slot.to_string())
                        .with_extension("save")
                })
        })
    }

    /// Save the current state of the console into a save file
    #[cfg(target_arch = "wasm32")]
    pub fn save_state(&mut self, _slot: u8) {
        // TODO: save to local storage or indexdb
    }

    /// Save the current state of the console into a save file.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_state(&mut self, slot: u8) {
        // Avoid saving any test roms
        if self.config.rom_path.to_string_lossy().contains("test") {
            return;
        }
        let cpu = self.control_deck.cpu();
        if let Some(save_path) = self.save_path(slot) {
            match bincode::serialize(&cpu)
                .context("failed to serialize save state")
                .map(|data| save_data(save_path, &data))
            {
                Ok(_) => self.add_message(format!("Saved state: Slot {slot}")),
                Err(err) => {
                    log::error!("{:?}", err);
                    self.add_message(format!("Failed to save slot {slot}"));
                }
            }
        }
    }

    /// Load the console with data saved from a save state
    #[cfg(target_arch = "wasm32")]
    pub fn load_state(&mut self, _slot: u8) {
        // TODO: load from local storage or indexdb
    }

    /// Load the console with data saved from a save state
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_state(&mut self, slot: u8) {
        if let Some(save_path) = self.save_path(slot) {
            if save_path.exists() {
                match load_data(save_path).and_then(|data| {
                    bincode::deserialize(&data)
                        .context("failed to deserialize load state")
                        .map(|cpu| self.control_deck.load_cpu(cpu))
                }) {
                    Ok(_) => self.add_message(format!("Loaded state: Slot {slot}")),
                    Err(err) => {
                        log::error!("{:?}", err);
                        self.add_message(format!("Failed to load slot {slot}"));
                    }
                }
            } else {
                self.add_message(format!("No save state found for slot {slot}"));
            }
        }
    }

    pub fn save_screenshot(&mut self) {
        // TODO: Provide download file for WASM
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::ppu::Ppu;
            use chrono::Local;

            let filename = PathBuf::from(
                Local::now()
                    .format("screenshot_%Y-%m-%d_at_%H_%M_%S")
                    .to_string(),
            )
            .with_extension("png");
            let image = image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
                Ppu::WIDTH,
                Ppu::HEIGHT,
                self.control_deck.frame_buffer(),
            )
            .expect("valid frame buffer");

            match image.save(&filename) {
                Ok(()) => self.add_message(filename.to_string_lossy()),
                Err(err) => {
                    log::error!("{err:?}");
                    self.add_message("Failed to save screenshot");
                }
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
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
