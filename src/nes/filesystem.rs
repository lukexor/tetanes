use super::{Nes, NesResult};
use crate::{
    common::Regional,
    nes::{menu::Menu, state::Mode, PauseMode},
};
use anyhow::Context;
use flate2::{bufread::DeflateDecoder, write::DeflateEncoder, Compression};
use std::{
    ffi::OsStr,
    fs::File,
    io::{BufReader, Read, Write},
    path::Path,
};

#[cfg(not(target_arch = "wasm32"))]
const SAVE_FILE_MAGIC_LEN: usize = 8;
#[cfg(not(target_arch = "wasm32"))]
const SAVE_FILE_MAGIC: [u8; SAVE_FILE_MAGIC_LEN] = *b"TETANES\x1a";
#[cfg(not(target_arch = "wasm32"))]
const MAJOR_VERSION: &str = env!("CARGO_PKG_VERSION_MAJOR");

/// Writes a header including a magic string and a version
///
/// # Errors
///
/// If the header fails to write to disk, then an error is returned.
#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
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
            File::create(path).with_context(|| format!("failed to create file {path:?}"))?,
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
            File::open(path).with_context(|| format!("failed to open file {path:?}"))?,
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
    let mut reader =
        BufReader::new(File::open(path).with_context(|| format!("Failed to open file {path:?}"))?);
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

#[inline]
pub(crate) fn filename(path: &Path) -> &str {
    path.file_name().and_then(OsStr::to_str).unwrap_or_else(|| {
        log::warn!("invalid rom_path: {path:?}");
        "??"
    })
}

impl Nes {
    pub fn initialize(&mut self, event_tx: crossbeam::channel::Sender<super::EventMsg>) {
        // Configure emulation based on config
        self.update_frame_rate();

        if self.config.zapper {
            self.window.set_cursor_visible(false);
        }

        for code in self.config.genie_codes.clone() {
            if let Err(err) = self.control_deck.add_genie_code(code.clone()) {
                log::warn!("{}", err);
                self.add_message(format!("Invalid Genie Code: '{code}'"));
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        if self.config.rom_path.is_dir() {
            self.mode = Mode::InMenu(Menu::LoadRom);
        } else {
            self.load_rom_path(self.config.rom_path.clone());
        }

        #[cfg(target_arch = "wasm32")]
        {
            use super::EventMsg;
            use wasm_bindgen::{closure::Closure, JsCast};

            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body().map(|body| (doc, body)))
                .map(|(doc, body)| {
                    let load_rom_tx = event_tx.clone();
                    let handle_load_rom = Closure::<dyn Fn()>::new(move || {
                        if let Err(err) = load_rom_tx.try_send(EventMsg::LoadRom) {
                            log::error!("failed to send load rom message to event_loop: {err:?}");
                        }
                    });

                    let load_rom_btn = doc.create_element("button").expect("created button");
                    load_rom_btn.set_text_content(Some("Load ROM"));
                    load_rom_btn
                        .add_event_listener_with_callback(
                            "click",
                            handle_load_rom.as_ref().unchecked_ref(),
                        )
                        .expect("added event listener");
                    body.append_child(&load_rom_btn).ok();
                    handle_load_rom.forget();

                    let pause_tx = event_tx.clone();
                    let handle_pause = Closure::<dyn Fn()>::new(move || {
                        if let Err(err) = pause_tx.try_send(EventMsg::Pause) {
                            log::error!("failed to send pause message to event_loop: {err:?}");
                        }
                    });

                    let pause_btn = doc.create_element("button").expect("created button");
                    pause_btn.set_text_content(Some("Pause"));
                    pause_btn
                        .add_event_listener_with_callback(
                            "click",
                            handle_pause.as_ref().unchecked_ref(),
                        )
                        .expect("added event listener");
                    body.append_child(&pause_btn).ok();
                    handle_pause.forget();
                })
                .expect("couldn't append canvas to document body");
        }
    }

    /// Loads a ROM cartridge into memory from a path.
    pub fn load_rom_path(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        let filename = filename(path);
        match File::open(path).with_context(|| format!("failed to open rom {path:?}")) {
            Ok(mut rom) => self.load_rom(filename, &mut rom),
            Err(err) => {
                log::error!("{path:?}: {err:?}");
                self.mode = Mode::InMenu(Menu::LoadRom);
                self.error = Some(format!("Failed to open ROM {filename:?}"));
            }
        }
    }

    /// Loads a ROM cartridge into memory from a reader.
    #[inline]
    pub fn load_rom(&mut self, filename: &str, rom: &mut impl Read) {
        self.pause_play(PauseMode::Manual);
        match self.control_deck.load_rom(filename, rom) {
            Ok(()) => {
                self.error = None;
                self.window.set_title(&filename.replace(".nes", ""));
                self.config.region = self.control_deck.region();
                if let Err(err) = self.load_sram() {
                    log::error!("{:?}: {:?}", self.config.rom_path, err);
                    self.add_message("Failed to load game state");
                }
                self.resume_play();
            }
            Err(err) => {
                log::error!("{:?}, {:?}", self.config.rom_path, err);
                self.mode = Mode::InMenu(Menu::LoadRom);
                self.error = Some(format!("Failed to load ROM {filename:?}"));
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(path) = self.save_path(1) {
                if path.exists() {
                    self.load_state(1);
                }
            }
            self.load_replay();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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
