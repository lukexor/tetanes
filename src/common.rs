//! Utils and Traits shared among modules

use crate::{
    nes_err,
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    serialization::Savable,
    NesResult,
};
use enum_dispatch::enum_dispatch;
use std::{
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
};

pub type Addr = u16;
pub type Word = usize;
pub type Byte = u8;
pub const CONFIG_DIR: &str = ".tetanes";

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NesFormat {
    Ntsc,
    Pal,
    Dendy,
}

#[enum_dispatch(MapperType)]
pub trait Powered {
    fn power_on(&mut self) {}
    fn power_off(&mut self) {}
    fn reset(&mut self) {}
    fn power_cycle(&mut self) {
        self.reset();
        self.power_off();
        self.power_on();
    }
}

#[enum_dispatch(MapperType)]
pub trait Clocked {
    fn clock(&mut self) -> usize {
        0
    }
}

#[macro_export]
macro_rules! hashmap {
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = HashMap::new();
            $(
                m.insert($key, $value);
            )+
            m
        }
    };
    ($hm:ident, { $($key:expr => $value:expr),+ } ) => (
        {
            $(
                $hm.insert($key, $value);
            )+
        }
    );
}

impl Savable for NesFormat {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => NesFormat::Ntsc,
            1 => NesFormat::Pal,
            2 => NesFormat::Dendy,
            _ => panic!("invalid NesFormat value"),
        };
        Ok(())
    }
}

/// Returns the users current HOME directory (if one exists)
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
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
pub fn create_png<P: AsRef<Path>>(png_path: &P, pixels: &[u8]) -> NesResult<String> {
    let png_path = png_path.as_ref();
    let png_file = std::fs::File::create(&png_path);
    if png_file.is_err() {
        return nes_err!(
            "failed to create png file {:?}: {}",
            png_path.display(),
            png_file.err().unwrap(),
        );
    }
    let png_file = BufWriter::new(png_file.unwrap()); // Safe to unwrap
    let mut png = png::Encoder::new(png_file, RENDER_WIDTH, RENDER_HEIGHT);
    png.set_color(png::ColorType::RGB);
    let writer = png.write_header();
    if let Err(e) = writer {
        return nes_err!("failed to save screenshot {:?}: {}", png_path.display(), e);
    }
    let result = writer.unwrap().write_image_data(&pixels);
    if let Err(e) = result {
        return nes_err!("failed to save screenshot {:?}: {}", png_path.display(), e);
    }
    Ok(format!("{}", png_path.display()))
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
