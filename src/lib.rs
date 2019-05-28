//! # Summary
//!
//! RustyNES is an emulator for the Nintendo Entertainment System (NES) released in 1983, written
//! using Rust and SDL2.
//!
//! It started as a personal curiosity that turned into a project for two classes to demonstrate
//! a proficiency in Rust and in digital sound production. It is still a work-in-progress, but
//! I hope to transform it into a fully-featured NES emulator that can play most games. It is my
//! hope to see a Rust emulator rise in popularity and compete with the more popular C and C++
//! versions.
//!
//! RustyNES is also meant to showcase how clean and readable low-level Rust programs can be in
//! addition to them having the type and memory-safety guarantees that Rust is known for.

pub mod cartridge;
pub mod console;
pub mod disasm;
pub mod filter;
pub mod input;
pub mod mapper;
pub mod memory;
pub mod serialization;
pub mod ui;

pub mod util {
    //! Various utility functions for the UI and Console

    use crate::console::{Image, SCREEN_HEIGHT, SCREEN_WIDTH};
    use chrono::prelude::*;
    use dirs;
    use image::{png, ColorType};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::Read;
    use std::path::{Path, PathBuf};

    /// Alias for Result<T, failure::Error>
    pub type Result<T> = std::result::Result<T, failure::Error>;

    const CONFIG_DIR: &str = ".rustynes";

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

    /// Returns the path where ROM thumbnails have been downloaded to
    ///
    /// # Arguments
    ///
    /// * `path` - An object that implements AsRef<Path> that holds the path to the currently
    /// running ROM
    ///
    /// # Errors
    ///
    /// Panics if path is not a valid path
    pub fn thumbnail_path<P: AsRef<Path>>(path: &P) -> Result<PathBuf> {
        let filehash = hash_file(path)?;
        let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
        path.push(CONFIG_DIR);
        path.push("thumbnail");
        path.push(filehash);
        path.set_extension("png");
        Ok(path)
    }

    /// Returns a SHA256 hash of the first 255 bytes of a file to uniquely identify it
    ///
    /// # Arguments
    ///
    /// * `path` - An object that implements AsRef<Path> that holds the path to the currently
    /// running ROM
    ///
    /// # Errors
    ///
    /// Panics if path is not a valid path or if there are permissions issues reading the file
    pub fn hash_file<P: AsRef<Path>>(path: &P) -> Result<String> {
        let mut file = fs::File::open(path)?;
        let mut buf = [0u8; 255];
        file.read_exact(&mut buf)?;
        Ok(format!("{:x}", Sha256::digest(&buf)))
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
    pub fn screenshot(pixels: &Image) {
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
    pub fn create_png<P: AsRef<Path>>(png_path: &P, pixels: &Image) {
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
        let png = png::PNGEncoder::new(png_file.unwrap());
        let encode = png.encode(
            pixels,
            SCREEN_WIDTH as u32,
            SCREEN_HEIGHT as u32,
            ColorType::RGB(8),
        );
        if encode.is_err() {
            eprintln!(
                "failed to save screenshot {:?}: {}",
                png_path.display(),
                encode.err().unwrap(),
            );
        }
        eprintln!("{}", png_path.display());
    }
}
