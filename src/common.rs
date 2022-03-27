//! Utils and Traits shared among modules

use crate::{
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    NesResult,
};
use enum_dispatch::enum_dispatch;
use pix_engine::prelude::{Image, PixelFormat};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONFIG_DIR: &str = ".config/tetanes";
pub const SAVE_DIR: &str = "save";
pub const SRAM_DIR: &str = "sram";

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NesFormat {
    Ntsc,
    Pal,
    Dendy,
}

#[enum_dispatch(Mapper)]
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

#[enum_dispatch(Mapper)]
pub trait Clocked {
    fn clock(&mut self) -> usize {
        0
    }
}

#[macro_export]
macro_rules! hashmap {
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = ::std::collections::HashMap::new();
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

pub(crate) fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("./"))
        .join(CONFIG_DIR)
}

pub(crate) fn config_path<P: AsRef<Path>>(path: P) -> PathBuf {
    config_dir().join(path)
}

/// Creates a '.png' file
///
/// # Arguments
///
/// * `png_path` - An object that implements [`AsRef<Path>`] for the location to save the `.png`
/// file
/// * `pixels` - An array of pixel data to save in `.png` format
///
/// # Errors
///
/// It's possible for this method to fail, but instead of erroring the program,
/// it'll simply log the error out to STDERR
pub fn create_png<P: AsRef<Path>>(png_path: &P, pixels: &[u8]) -> NesResult<()> {
    Image::from_bytes(RENDER_WIDTH, RENDER_HEIGHT, pixels, PixelFormat::Rgb)?.save(png_path)?;
    Ok(())
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
                line.push(' ');
            }
        }

        if line_len > 0 {
            line.push_str("  |");
            for c in line_data {
                if (*c as char).is_ascii() && !(*c as char).is_control() {
                    line.push_str(&format!("{}", (*c as char)));
                } else {
                    line.push('.');
                }
            }
            line.push('|');
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
