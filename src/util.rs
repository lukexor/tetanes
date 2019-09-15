//! Various utility functions for the UI and Console

use crate::console::{RENDER_HEIGHT, RENDER_WIDTH};
use crate::serialization::Savable;
use crate::{nes_err, Result};
use chrono::prelude::{DateTime, Local};
use dirs;
use image::{png, ColorType, Pixel};
use std::fs;
use std::io::{Read, Write};
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
pub fn find_roms<P: AsRef<Path>>(path: P) -> Result<Vec<PathBuf>> {
    use std::ffi::OsStr;
    let path = path.as_ref();
    let mut roms = Vec::new();
    if path.is_dir() {
        path.read_dir()
            .map_err(|e| nes_err!("unable to read directory {:?}: {}", path, e))?
            .filter_map(|f| f.ok())
            .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
            .for_each(|f| roms.push(f.path()));
    } else if path.is_file() {
        roms.push(path.to_path_buf());
    } else {
        Err(nes_err!("invalid path: {:?}", path))?;
    }
    if roms.is_empty() {
        Err(nes_err!("no rom files found or specified"))?;
    }
    Ok(roms)
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
pub fn sram_path<P: AsRef<Path>>(path: &P) -> Result<PathBuf> {
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
pub fn save_path<P: AsRef<Path>>(path: &P, slot: u8) -> Result<PathBuf> {
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
    let mut png_path = PathBuf::from(format!(
        "screenshot_{}",
        datetime.format("%Y-%m-%dT%H-%M-%S").to_string()
    ));
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
    let png = png::PNGEncoder::new(png_file.unwrap()); // Safe to unwrap
    let encode = png.encode(
        pixels,
        RENDER_WIDTH as u32,
        RENDER_HEIGHT as u32,
        ColorType::RGB(8),
    );
    if let Err(e) = encode {
        eprintln!("failed to save screenshot {:?}: {}", png_path.display(), e);
        return;
    }
    println!("{}", png_path.display());
}

/// Writes a header including a magic string and a version
pub fn write_save_header(fh: &mut dyn Write) -> Result<()> {
    SAVE_FILE_MAGIC.save(fh)?;
    VERSION.save(fh)
}

/// Validates a file to ensure it matches the current version and magic
pub fn validate_save_header(fh: &mut dyn Read) -> Result<()> {
    let mut magic = [0u8; 9];
    magic.load(fh)?;
    if magic != SAVE_FILE_MAGIC {
        Err(nes_err!("invalid save file format"))?;
    }
    let mut version = 0u8;
    version.load(fh)?;
    if version != VERSION {
        Err(nes_err!(
            "invalid save file version. current: {}, save file: {}",
            VERSION,
            version,
        ))?;
    }
    Ok(())
}

pub struct WindowIcon {
    pub width: u32,
    pub height: u32,
    pub pitch: u32, // Number of pixels per row
    pub pixels: Vec<u8>,
}

impl WindowIcon {
    /// Loads pixel values for an image icon
    pub fn load() -> Result<Self> {
        let image = image::open(&ICON_PATH)?.to_rgb();
        let (width, height) = image.dimensions();
        let mut pixels = Vec::with_capacity((width * height * 3) as usize);
        for pixel in image.pixels() {
            pixels.extend_from_slice(pixel.channels());
        }
        Ok(Self {
            width,
            height,
            pitch: width * 3,
            pixels,
        })
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
