use crate::{NesError, NesResult};
use anyhow::anyhow;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

pub const CONFIG_DIR: &str = ".config/tetanes";
pub const SAVE_DIR: &str = "save";
pub const SRAM_DIR: &str = "sram";

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum NesRegion {
    Ntsc,
    Pal,
    Dendy,
}

impl NesRegion {
    pub const fn as_slice() -> &'static [Self] {
        &[NesRegion::Ntsc, NesRegion::Pal, NesRegion::Dendy]
    }
}

impl Default for NesRegion {
    fn default() -> Self {
        Self::Ntsc
    }
}

impl AsRef<str> for NesRegion {
    fn as_ref(&self) -> &str {
        match self {
            Self::Ntsc => "NTSC",
            Self::Pal => "PAL",
            Self::Dendy => "Dendy",
        }
    }
}

impl TryFrom<&str> for NesRegion {
    type Error = NesError;

    fn try_from(value: &str) -> NesResult<Self> {
        match value {
            "NTSC" => Ok(Self::Ntsc),
            "PAL" => Ok(Self::Pal),
            "Dendy" => Ok(Self::Dendy),
            _ => Err(anyhow!("invalid nes region")),
        }
    }
}

impl From<usize> for NesRegion {
    fn from(value: usize) -> Self {
        match value {
            1 => Self::Pal,
            2 => Self::Dendy,
            _ => Self::Ntsc,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Kind {
    Soft,
    Hard,
}

#[enum_dispatch(Mapper)]
pub trait Reset {
    fn reset(&mut self, _kind: Kind) {}
}

#[enum_dispatch(Mapper)]
pub trait Clock {
    fn clock(&mut self) -> usize {
        0
    }
    fn clock_to(&mut self, _clocks: u64) {}
}

#[macro_export]
macro_rules! hashmap {
    { $($key:expr => $value:expr),* $(,)? } => {{
        let mut m = ::std::collections::HashMap::new();
        $(
            m.insert($key, $value);
        )*
        m
    }};
    ($hm:ident, { $($key:expr => $value:expr),* $(,)? } ) => ({
        $(
            $hm.insert($key, $value);
        )*
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("./"))
        .join(CONFIG_DIR)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn config_path<P: AsRef<Path>>(path: P) -> PathBuf {
    config_dir().join(path)
}

/// Prints a hex dump of a given byte array starting at `addr_offset`.
#[must_use]
pub fn hexdump(data: &[u8], addr_offset: usize) -> Vec<String> {
    use std::cmp;

    let mut addr = 0;
    let len = data.len();
    let mut last_line_same = false;
    let mut output = Vec::new();
    let mut last_line = String::with_capacity(80);
    while addr <= len {
        let end = cmp::min(addr + 16, len);
        let line_data = &data[addr..end];
        let line_len = line_data.len();

        let mut line = String::with_capacity(80);
        for byte in line_data.iter() {
            let _ = write!(line, " {:02X}", byte);
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
                    let _ = write!(line, "{}", (*c as char));
                } else {
                    line.push('.');
                }
            }
            line.push('|');
        }
        if last_line == line {
            if !last_line_same {
                last_line_same = true;
                output.push("*".to_string());
            }
        } else {
            last_line_same = false;
            output.push(format!("{:08x} {}", addr + addr_offset, line));
        }
        last_line = line;

        addr += 16;
    }
    output
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::{
        common::{Kind, NesRegion, Reset},
        control_deck::ControlDeck,
        input::{GamepadBtn, GamepadSlot},
        mapper::{Mapper, MapperRevision},
        nes::event::{Action, NesState, Setting},
        ppu::{VideoFilter, RENDER_HEIGHT, RENDER_WIDTH},
    };
    use anyhow::Context;
    use once_cell::sync::Lazy;
    use pix_engine::prelude::{Image, PixelFormat};
    use serde::{Deserialize, Serialize};
    use std::fmt::Write;
    use std::{
        collections::hash_map::DefaultHasher,
        env,
        fs::{self, File},
        hash::{Hash, Hasher},
        io::{BufReader, BufWriter},
        path::{Path, PathBuf},
    };

    pub(crate) const RESULT_DIR: &str = "test_results";

    static INIT_TESTS: Lazy<bool> = Lazy::new(|| {
        let result_dir = PathBuf::from(RESULT_DIR);
        if result_dir.exists() {
            fs::remove_dir_all(result_dir).expect("cleared test results dir");
        }
        true
    });
    static PASS_DIR: Lazy<PathBuf> = Lazy::new(|| {
        let directory = PathBuf::from(RESULT_DIR).join("pass");
        fs::create_dir_all(&directory).expect("created pass test results dir");
        directory
    });
    static FAIL_DIR: Lazy<PathBuf> = Lazy::new(|| {
        let directory = PathBuf::from(RESULT_DIR).join("fail");
        fs::create_dir_all(&directory).expect("created fail test results dir");
        directory
    });

    #[macro_export]
    macro_rules! test_roms {
        ($directory:expr, $( $(#[ignore = $reason:expr])? $test:ident ),* $(,)?) => {$(
            $(#[ignore = $reason])?
            #[test]
            fn $test() {
                $crate::common::tests::test_rom($directory, stringify!($test));
            }
        )*};
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[must_use]
    struct TestFrame {
        number: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hash: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        slot: Option<GamepadSlot>,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<Action>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[must_use]
    struct RomTest {
        name: String,
        frames: Vec<TestFrame>,
    }

    fn get_rom_tests(directory: &str) -> (PathBuf, Vec<RomTest>) {
        let file = PathBuf::from(directory)
            .join("tests")
            .with_extension("json");
        let tests = File::open(&file)
            .and_then(|file| {
                Ok(serde_json::from_reader::<_, Vec<RomTest>>(BufReader::new(
                    file,
                ))?)
            })
            .expect("valid rom test data");
        (file, tests)
    }

    fn load_control_deck<P: AsRef<Path>>(path: P) -> ControlDeck {
        let path = path.as_ref();
        let mut rom = BufReader::new(File::open(path).unwrap());
        let mut deck = ControlDeck::default();
        deck.load_rom(&path.to_string_lossy(), &mut rom).unwrap();
        deck.set_filter(VideoFilter::Pixellate);
        deck.set_region(NesRegion::Ntsc);
        deck
    }

    fn handle_frame_action(test_frame: &TestFrame, deck: &mut ControlDeck) {
        if let Some(action) = test_frame.action {
            log::debug!("{:?}", action);
            match action {
                Action::Nes(state) => match state {
                    NesState::SoftReset => deck.reset(Kind::Soft),
                    NesState::HardReset => deck.reset(Kind::Hard),
                    NesState::MapperRevision(board) => match board {
                        MapperRevision::Mmc3(revision) => {
                            if let Mapper::Txrom(ref mut mapper) = deck.cart_mut().mapper {
                                mapper.set_revision(revision);
                            }
                        }
                        _ => panic!("unhandled MapperRevision {:?}", board),
                    },
                    _ => panic!("unhandled Nes state: {:?}", state),
                },
                Action::Setting(setting) => match setting {
                    Setting::SetVideoFilter(filter) => deck.set_filter(filter),
                    Setting::SetNesFormat(format) => deck.set_region(format),
                    _ => panic!("unhandled Setting: {:?}", setting),
                },
                Action::Gamepad(button) => {
                    let slot = test_frame.slot.unwrap_or(GamepadSlot::One);
                    let mut gamepad = deck.gamepad_mut(slot);
                    match button {
                        GamepadBtn::Left => gamepad.left = true,
                        GamepadBtn::Right => gamepad.right = true,
                        GamepadBtn::Up => gamepad.up = true,
                        GamepadBtn::Down => gamepad.down = true,
                        GamepadBtn::A => gamepad.a = true,
                        GamepadBtn::B => gamepad.b = true,
                        GamepadBtn::Select => gamepad.select = true,
                        GamepadBtn::Start => gamepad.start = true,
                        _ => panic!("unhandled Gamepad button: {:?}", button),
                    };
                }
                _ => (),
            }
        }
    }

    fn handle_snapshot(
        test: &str,
        test_frame: &TestFrame,
        deck: &mut ControlDeck,
        count: usize,
    ) -> Option<(u64, u64, u32, PathBuf)> {
        test_frame.hash.map(|expected| {
            let mut hasher = DefaultHasher::new();
            let frame = deck.frame_buffer();
            frame.hash(&mut hasher);
            let actual = hasher.finish();
            log::debug!(
                "frame : {}, matched: {}",
                test_frame.number,
                expected == actual
            );

            let result_dir = if env::var("UPDATE_SNAPSHOT").is_ok() || expected == actual {
                &*PASS_DIR
            } else {
                &*FAIL_DIR
            };
            let mut filename = test.to_owned();
            if let Some(ref name) = test_frame.name {
                let _ = write!(filename, "_{}", name);
            } else if count > 0 {
                let _ = write!(filename, "_{}", count + 1);
            }
            let screenshot = result_dir
                .join(PathBuf::from(filename))
                .with_extension("png");

            Image::from_bytes(RENDER_WIDTH, RENDER_HEIGHT, frame, PixelFormat::Rgba)
                .expect("valid frame")
                .save(&screenshot)
                .expect("result screenshot");

            (expected, actual, test_frame.number, screenshot)
        })
    }

    pub(crate) fn test_rom(directory: &str, test_name: &str) {
        if !&*INIT_TESTS {
            log::debug!("Initialized tests");
        }

        let (test_file, mut tests) = get_rom_tests(directory);
        let mut test = tests.iter_mut().find(|test| test.name.eq(test_name));
        assert!(test.is_some(), "No test found matching {:?}", test_name);
        let test = test.as_mut().unwrap();

        let rom = PathBuf::from(directory)
            .join(PathBuf::from(&test.name))
            .with_extension("nes");
        assert!(rom.exists(), "No test rom found for {:?}", rom);

        let mut deck = load_control_deck(&rom);
        if env::var("RUST_LOG").is_ok() {
            let _ = pretty_env_logger::try_init();
            deck.cpu_mut().debugging = true;
        }

        let mut results = Vec::new();
        for test_frame in test.frames.iter() {
            log::debug!(
                "{} - {:?}",
                test_frame.number,
                deck.gamepad_mut(GamepadSlot::One)
            );

            while deck.frame_number() < test_frame.number {
                deck.clock_frame().expect("valid frame clock");
                deck.clear_audio_samples();
                deck.gamepad_mut(GamepadSlot::One).clear();
                deck.gamepad_mut(GamepadSlot::Two).clear();
            }

            handle_frame_action(test_frame, &mut deck);
            if let Some(result) = handle_snapshot(&test.name, test_frame, &mut deck, results.len())
            {
                results.push(result);
            }
        }
        let mut update_required = false;
        for (mut expected, actual, frame_number, screenshot) in results {
            if env::var("UPDATE_SNAPSHOT").is_ok() && expected != actual {
                expected = actual;
                update_required = true;
                if let Some(ref mut frame) = test
                    .frames
                    .iter_mut()
                    .find(|frame| frame.number == frame_number)
                {
                    frame.hash = Some(actual);
                }
            }
            assert_eq!(
                expected, actual,
                "mismatched snapshot for {:?} -> {:?}",
                rom, screenshot
            );
        }
        if update_required {
            File::create(&test_file)
                .context("failed to open rom test file")
                .and_then(|file| {
                    serde_json::to_writer_pretty(BufWriter::new(file), &tests)
                        .context("failed to serialize rom data")
                })
                .expect("failed to update snapshot");
        }
    }
}
