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
pub mod util;
