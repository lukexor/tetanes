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

#[cfg(test)]
pub(crate) mod tests {
    use crate::{
        common::Powered,
        control_deck::ControlDeck,
        input::GamepadSlot,
        memory::RamState,
        ppu::{VideoFilter, RENDER_HEIGHT, RENDER_WIDTH},
    };
    use pix_engine::prelude::{Image, PixelFormat};
    use std::{
        collections::hash_map::DefaultHasher,
        fs::{self, File},
        hash::{Hash, Hasher},
        io::BufReader,
        path::{Path, PathBuf},
    };

    pub(crate) const SLOT1: GamepadSlot = GamepadSlot::One;
    pub(crate) const TEST_DIR: &str = "test_roms";

    pub(crate) fn load<P: AsRef<Path>>(path: P) -> ControlDeck {
        let path = path.as_ref();
        let mut deck = ControlDeck::new(RamState::AllZeros);
        deck.set_filter(VideoFilter::None);
        let rom = File::open(path).unwrap();
        let mut rom = BufReader::new(rom);
        deck.load_rom(&path.to_string_lossy(), &mut rom).unwrap();
        deck.power_on();
        deck
    }

    pub(crate) fn compare(expected_hash: u64, frame: &[u8], test: &str) {
        let mut hasher = DefaultHasher::new();
        frame.hash(&mut hasher);
        let actual_hash = hasher.finish();
        let results_dir = PathBuf::from("test_results");
        let screenshot_path = results_dir.join(PathBuf::from(test)).with_extension("png");
        if expected_hash != actual_hash {
            if !results_dir.exists() {
                fs::create_dir(&results_dir).expect("created test results dir");
            }
            Image::from_bytes(RENDER_WIDTH, RENDER_HEIGHT, frame, PixelFormat::Rgb)
                .expect("valid frame")
                .save(screenshot_path)
                .expect("failure screenshot");
        } else if screenshot_path.exists() {
            let _ = fs::remove_file(screenshot_path);
        }
        assert_eq!(expected_hash, actual_hash, "mismatched {}.png", test);
    }

    pub(crate) fn test_rom<P: AsRef<Path>>(rom: P, run_frames: i32, expected_hash: u64) {
        let rom = rom.as_ref();
        let mut deck = load(PathBuf::from(TEST_DIR).join(rom));
        for _ in 0..=run_frames {
            deck.clock_frame();
        }
        let frame = deck.frame_buffer();
        let test = rom.file_stem().expect("valid test file").to_string_lossy();
        compare(expected_hash, frame, &test);
    }

    pub(crate) fn test_rom_advanced<P, F>(rom: P, run_frames: i32, f: F)
    where
        P: AsRef<Path>,
        F: Fn(i32, &mut ControlDeck),
    {
        let rom = rom.as_ref();
        let mut deck = load(PathBuf::from(TEST_DIR).join(rom));
        for frame in 0..=run_frames {
            f(frame, &mut deck);
            deck.clock_frame();
        }
    }
}
