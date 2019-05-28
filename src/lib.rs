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
    use crate::console::{Image, SCREEN_HEIGHT, SCREEN_WIDTH};
    use chrono::prelude::*;
    use dirs;
    use image::{png, ColorType};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::Read;
    use std::path::{Path, PathBuf};

    pub type Result<T> = std::result::Result<T, failure::Error>;

    const CONFIG_DIR: &str = ".rustynes";

    pub fn sram_path<P: AsRef<Path>>(path: &P) -> Result<PathBuf> {
        let save_name = path.as_ref().file_stem().and_then(|s| s.to_str()).unwrap();
        let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
        path.push(CONFIG_DIR);
        path.push("sram");
        path.push(save_name);
        path.set_extension("dat");
        Ok(path)
    }

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

    pub fn thumbnail_path<P: AsRef<Path>>(path: &P) -> Result<PathBuf> {
        let filehash = hash_file(path)?;
        let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
        path.push(CONFIG_DIR);
        path.push("thumbnail");
        path.push(filehash);
        path.set_extension("png");
        Ok(path)
    }

    pub fn hash_file<P: AsRef<Path>>(path: &P) -> Result<String> {
        let mut file = fs::File::open(path)?;
        let mut buf = [0u8; 255];
        file.read_exact(&mut buf)?;
        Ok(format!("{:x}", Sha256::digest(&buf)))
    }

    pub fn home_dir() -> Option<PathBuf> {
        dirs::home_dir().and_then(|d| Some(d.to_path_buf()))
    }

    pub fn screenshot(pixels: &Image) {
        let datetime: DateTime<Local> = Local::now();
        let mut png_path = PathBuf::from(format!(
            "Screenshot {}",
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        ));
        png_path.set_extension("png");
        create_png(&png_path, pixels);
    }

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
