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

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct NesError(pub String);

#[macro_export]
macro_rules! nes_err {
    ($($arg:tt)*) => {
        crate::NesError(format!($($arg)*))
    };
}

pub fn to_nes_err(err: String) -> NesError {
    NesError(err)
}

impl fmt::Display for NesError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for NesError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{{ err: {}, file: {}, line: {} }}",
            self.0,
            file!(),
            line!()
        )
    }
}

impl std::error::Error for NesError {}
