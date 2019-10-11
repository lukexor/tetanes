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

use pix_engine::PixEngineErr;
use std::fmt;

pub mod cartridge;
pub mod console;
pub mod filter;
pub mod input;
pub mod mapper;
pub mod memory;
pub mod serialization;
pub mod ui;
pub mod util;

pub type NesResult<T> = std::result::Result<T, NesErr>;

pub struct NesErr {
    description: String,
}

impl NesErr {
    fn new<D: ToString>(desc: D) -> Self {
        Self {
            description: desc.to_string(),
        }
    }
    fn err<T, D: ToString>(desc: D) -> NesResult<T> {
        Err(Self {
            description: desc.to_string(),
        })
    }
}

#[macro_export]
macro_rules! nes_err {
    ($($arg:tt)*) => {
        crate::NesErr::err(&format!($($arg)*))
    };
}
#[macro_export]
macro_rules! map_nes_err {
    ($($arg:tt)*) => {
        crate::NesErr::new(&format!($($arg)*))
    };
}

impl fmt::Display for NesErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description)
    }
}

impl fmt::Debug for NesErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{{ err: {}, file: {}, line: {} }}",
            self.description,
            file!(),
            line!()
        )
    }
}

impl std::error::Error for NesErr {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl From<std::io::Error> for NesErr {
    fn from(err: std::io::Error) -> Self {
        Self {
            description: err.to_string(),
        }
    }
}

impl From<NesErr> for PixEngineErr {
    fn from(err: NesErr) -> Self {
        Self::new(&err.to_string())
    }
}

impl From<PixEngineErr> for NesErr {
    fn from(err: PixEngineErr) -> Self {
        Self::new(&err.to_string())
    }
}
