//! Various utility functions for the UI and Console

use crate::console::{RENDER_HEIGHT, RENDER_WIDTH};
use crate::serialization::Savable;
use crate::{map_nes_err, nes_err, NesResult};
use chrono::prelude::{DateTime, Local};
use dirs;
use png;
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const CONFIG_DIR: &str = ".rustynes";
const ICON_PATH: &str = "static/rustynes_icon.png";
const SAVE_FILE_MAGIC: [u8; 9] = *b"RUSTYNES\x1a";
// MAJOR version of SemVer. Increases when save file format isn't backwards compatible
const VERSION: u8 = 0;

/// Searches for valid NES rom files ending in `.nes`
///
/// If rom_path is a `.nes` file, uses that
/// If no arg[1], searches current directory for `.nes` files
pub fn find_roms<P: AsRef<Path>>(path: P) -> NesResult<Vec<PathBuf>> {
    use std::ffi::OsStr;
    let path = path.as_ref();
    let mut roms = Vec::new();
    if path.is_dir() {
        path.read_dir()
            .map_err(|e| map_nes_err!("unable to read directory {:?}: {}", path, e))?
            .filter_map(|f| f.ok())
            .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
            .for_each(|f| roms.push(f.path()));
    } else if path.is_file() {
        roms.push(path.to_path_buf());
    } else {
        nes_err!("invalid path: {:?}", path)?;
    }
    if roms.is_empty() {
        nes_err!("no rom files found or specified")
    } else {
        Ok(roms)
    }
}

/// Returns the path where battery-backed Save RAM files are stored
///
/// # Arguments
///
/// * `path` - An object that implements AsRef<Path> that holds the path to the currently
/// running ROM
///
/// # Errors
///
/// Panics if path is not a valid path
pub fn sram_path<P: AsRef<Path>>(path: &P) -> NesResult<PathBuf> {
    let save_name = path.as_ref().file_stem().and_then(|s| s.to_str()).unwrap();
    let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
    path.push(CONFIG_DIR);
    path.push("sram");
    path.push(save_name);
    path.set_extension("dat");
    Ok(path)
}

/// Returns the path where Save states are stored
///
/// # Arguments
///
/// * `path` - An object that implements AsRef<Path> that holds the path to the currently
/// running ROM
///
/// # Errors
///
/// Panics if path is not a valid path
pub fn save_path<P: AsRef<Path>>(path: &P, slot: u8) -> NesResult<PathBuf> {
    let save_name = path.as_ref().file_stem().and_then(|s| s.to_str()).unwrap();
    let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
    path.push(CONFIG_DIR);
    path.push("save");
    path.push(save_name);
    path.push(format!("{}", slot));
    path.set_extension("dat");
    Ok(path)
}

/// Returns the users current HOME directory (if one exists)
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir().and_then(|d| Some(d.to_path_buf()))
}

/// Takes a screenshot and saves it to the current directory as a `.png` file
///
/// # Arguments
///
/// * `pixels` - An array of pixel data to save in `.png` format
///
/// # Errors
///
/// It's possible for this method to fail, but instead of erroring the program,
/// it'll simply log the error out to STDERR
pub fn screenshot(pixels: &[u8]) {
    let datetime: DateTime<Local> = Local::now();
    let mut png_path = PathBuf::from(
        datetime
            .format("Screen Shot %Y-%m-%d at %H.%M.%S")
            .to_string(),
    );
    png_path.set_extension("png");
    create_png(&png_path, pixels);
}

/// Creates a '.png' file
///
/// # Arguments
///
/// * `png_path` - An object that implements AsRef<Path> for the location to save the `.png`
/// file
/// * `pixels` - An array of pixel data to save in `.png` format
///
/// # Errors
///
/// It's possible for this method to fail, but instead of erroring the program,
/// it'll simply log the error out to STDERR
pub fn create_png<P: AsRef<Path>>(png_path: &P, pixels: &[u8]) {
    let png_path = png_path.as_ref();
    let png_file = fs::File::create(&png_path);
    if png_file.is_err() {
        eprintln!(
            "failed to create png file {:?}: {}",
            png_path.display(),
            png_file.err().unwrap(),
        );
        return;
    }
    let png_file = BufWriter::new(png_file.unwrap());
    let mut png = png::Encoder::new(png_file, RENDER_WIDTH, RENDER_HEIGHT); // Safe to unwrap
    png.set_color(png::ColorType::RGB);
    let writer = png.write_header();
    if let Err(e) = writer {
        eprintln!("failed to save screenshot {:?}: {}", png_path.display(), e);
        return;
    }
    let result = writer.unwrap().write_image_data(&pixels);
    if let Err(e) = result {
        eprintln!("failed to save screenshot {:?}: {}", png_path.display(), e);
        return;
    }
    println!("{}", png_path.display());
}

/// Writes a header including a magic string and a version
pub fn write_save_header(fh: &mut dyn Write) -> NesResult<()> {
    SAVE_FILE_MAGIC.save(fh)?;
    VERSION.save(fh)
}

/// Validates a file to ensure it matches the current version and magic
pub fn validate_save_header(fh: &mut dyn Read) -> NesResult<()> {
    let mut magic = [0u8; 9];
    magic.load(fh)?;
    if magic != SAVE_FILE_MAGIC {
        nes_err!("invalid save file format")
    } else {
        let mut version = 0u8;
        version.load(fh)?;
        if version != VERSION {
            nes_err!(
                "invalid save file version. current: {}, save file: {}",
                VERSION,
                version,
            )
        } else {
            Ok(())
        }
    }
}

pub struct WindowIcon {
    pub width: u32,
    pub height: u32,
    pub pitch: u32, // Number of pixels per row
    pub pixels: Vec<u8>,
}

impl WindowIcon {
    /// Loads pixel values for an image icon
    pub fn load() -> NesResult<Self> {
        let icon_file = BufReader::new(fs::File::open(&ICON_PATH)?);
        let image = png::Decoder::new(icon_file);
        let (info, mut reader) = image
            .read_info()
            .map_err(|e| map_nes_err!("failed to read png info: {}", e))?;
        let mut pixels = vec![0; info.buffer_size()];
        reader
            .next_frame(&mut pixels)
            .map_err(|e| map_nes_err!("failed to read png: {}", e))?;
        Ok(Self {
            width: info.width,
            height: info.height,
            pitch: info.width * 4,
            pixels,
        })
    }
}

pub fn hexdump(data: &[u8], addr_offset: usize) {
    use std::cmp;

    let mut addr = 0;
    let len = data.len();
    let mut last_line_same = false;
    let mut last_line = String::with_capacity(80);
    while addr <= len {
        let end = cmp::min(addr + 16, len);
        let line_data = &data[addr..end];
        let line_len = line_data.len();

        let mut line = String::with_capacity(80);
        for byte in line_data.iter() {
            line.push_str(&format!(" {:02X}", byte));
        }

        if line_len % 16 > 0 {
            let words_left = (16 - line_len) / 2;
            for _ in 0..3 * words_left {
                line.push_str(" ");
            }
        }

        if line_len > 0 {
            line.push_str("  |");
            for c in line_data {
                if (*c as char).is_ascii() && !(*c as char).is_control() {
                    line.push_str(&format!("{}", (*c as char)));
                } else {
                    line.push_str(".");
                }
            }
            line.push_str("|");
        }
        if last_line == line {
            if !last_line_same {
                last_line_same = true;
                println!("*");
            }
        } else {
            last_line_same = false;
            println!("{:08x} {}", addr + addr_offset, line);
        }
        last_line = line;

        addr += 16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_header() {
        let mut file = fs::File::create("header.dat").unwrap();
        write_save_header(&mut file).unwrap();
    }

    #[test]
    fn test_find_roms() {
        let rom_tests = &[
            // (Test name, Path, Error)
            // CWD with no `.nes` files
            (
                "CWD with no nes files",
                "./",
                "no rom files found or specified",
            ),
            // Directory with no `.nes` files
            (
                "Dir with no nes files",
                "src/",
                "no rom files found or specified",
            ),
            (
                "invalid directory",
                "invalid/",
                "invalid path: \"invalid/\"",
            ),
        ];
        for test in rom_tests {
            let roms = find_roms(test.1);
            assert!(roms.is_err(), "invalid path {}", test.0);
            assert_eq!(
                roms.err().unwrap().to_string(),
                test.2,
                "error matches {}",
                test.0
            );
        }
    }
}
