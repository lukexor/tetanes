use anyhow::anyhow;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use tetanes_util::{NesError, NesResult};

pub const SAVE_DIR: &str = "save";
pub const SRAM_DIR: &str = "sram";

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum NesRegion {
    #[default]
    Ntsc,
    Pal,
    Dendy,
}

impl NesRegion {
    pub const fn as_slice() -> &'static [Self] {
        &[NesRegion::Ntsc, NesRegion::Pal, NesRegion::Dendy]
    }

    #[must_use]
    pub fn is_ntsc(&self) -> bool {
        self == &Self::Ntsc
    }

    #[must_use]
    pub fn is_pal(&self) -> bool {
        self == &Self::Pal
    }

    #[must_use]
    pub fn is_dendy(&self) -> bool {
        self == &Self::Dendy
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

#[enum_dispatch(Mapper)]
pub trait Regional {
    fn region(&self) -> NesRegion {
        NesRegion::default()
    }
    fn set_region(&mut self, _region: NesRegion) {}
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum ResetKind {
    Soft,
    Hard,
}

#[enum_dispatch(Mapper)]
pub trait Reset {
    fn reset(&mut self, _kind: ResetKind) {}
}

#[enum_dispatch(Mapper)]
pub trait Clock {
    fn clock(&mut self) -> usize {
        0
    }
    fn clock_to(&mut self, _clocks: u64) {}
}

/// Trait for types that can output `f32` audio samples.
pub trait AudioSample {
    fn output(&self) -> f32;
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
            let _ = write!(line, " {byte:02X}");
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
