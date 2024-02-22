#![doc = include_str!("../README.md")]
#![warn(
    clippy::all,
    future_incompatible,
    nonstandard_style,
    rust_2018_compatibility,
    rust_2018_idioms,
    rust_2021_compatibility,
    unused
)]
#![doc(
    html_favicon_url = "https://github.com/lukexor/tetanes/blob/main/static/tetanes_icon.png?raw=true",
    html_logo_url = "https://github.com/lukexor/tetanes/blob/main/static/tetanes_icon.png?raw=true"
)]

pub mod apu;
pub mod audio;
pub mod bus;
pub mod cart;
#[macro_use]
pub mod common;
pub mod control_deck;
pub mod cpu;
pub mod debugger;
pub mod filesystem;
pub mod genie;
pub mod input;
pub mod logging;
pub mod mapper;
pub mod mem;
pub mod nes;
pub mod platform;
pub mod ppu;
pub mod profiling;
pub mod video;

pub type NesError = anyhow::Error;
pub type NesResult<T> = anyhow::Result<T, NesError>;
