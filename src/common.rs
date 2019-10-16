//! Utils and Traits shared among modules

use crate::{
    nes_err,
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    NesResult,
};
use dirs;
use png;
use std::{
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
};

pub const CONFIG_DIR: &str = ".rustynes";

pub trait Powered {
    fn reset(&mut self) {}
    fn power_cycle(&mut self) {
        self.reset();
    }
}

pub trait Clocked {
    fn clock(&mut self) -> u64 {
        0
    }
}

/// Returns the users current HOME directory (if one exists)
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir().and_then(|d| Some(d.to_path_buf()))
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
    let png_file = fs::File::create(&png_path);
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

// use std::convert::TryInto;
// use std::ops::{Add, AddAssign, Sub, SubAssign};
// use wasm_bindgen::prelude::*;

// pub use std::time::*;

// #[cfg(not(target_arch = "wasm32"))]
// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
// pub struct Instant(std::time::Instant);
// #[cfg(not(target_arch = "wasm32"))]
// impl Instant {
//     pub fn now() -> Self {
//         Self(std::time::Instant::now())
//     }
//     pub fn duration_since(&self, earlier: Instant) -> Duration {
//         self.0.duration_since(earlier.0)
//     }
//     pub fn elapsed(&self) -> Duration {
//         self.0.elapsed()
//     }
//     pub fn checked_add(&self, duration: Duration) -> Option<Self> {
//         self.0.checked_add(duration).map(|i| Self(i))
//     }
//     pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
//         self.0.checked_sub(duration).map(|i| Self(i))
//     }
// }

// #[cfg(target_arch = "wasm32")]
// #[wasm_bindgen]
// extern "C" {
//     #[wasm_bindgen(js_namespace = Date, js_name = now)]
//     fn date_now() -> f64;
// }
// #[cfg(target_arch = "wasm32")]
// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
// pub struct Instant(u64);
// #[cfg(target_arch = "wasm32")]
// impl Instant {
//     pub fn now() -> Self {
//         Self(date_now() as u64)
//     }
//     pub fn duration_since(&self, earlier: Instant) -> Duration {
//         Duration::from_millis(self.0 - earlier.0)
//     }
//     pub fn elapsed(&self) -> Duration {
//         Self::now().duration_since(*self)
//     }
//     pub fn checked_add(&self, duration: Duration) -> Option<Self> {
//         match duration.as_millis().try_into() {
//             Ok(duration) => self.0.checked_add(duration).map(|i| Self(i)),
//             Err(_) => None,
//         }
//     }
//     pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
//         match duration.as_millis().try_into() {
//             Ok(duration) => self.0.checked_sub(duration).map(|i| Self(i)),
//             Err(_) => None,
//         }
//     }
// }

// impl Add<Duration> for Instant {
//     type Output = Instant;
//     fn add(self, other: Duration) -> Instant {
//         self.checked_add(other).unwrap()
//     }
// }
// impl Sub<Duration> for Instant {
//     type Output = Instant;
//     fn sub(self, other: Duration) -> Instant {
//         self.checked_sub(other).unwrap()
//     }
// }
// impl Sub<Instant> for Instant {
//     type Output = Duration;
//     fn sub(self, other: Instant) -> Duration {
//         self.duration_since(other)
//     }
// }
// impl AddAssign<Duration> for Instant {
//     fn add_assign(&mut self, other: Duration) {
//         *self = *self + other;
//     }
// }
// impl SubAssign<Duration> for Instant {
//     fn sub_assign(&mut self, other: Duration) {
//         *self = *self - other;
//     }
// }
