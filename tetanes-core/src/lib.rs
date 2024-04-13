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

pub mod action;
pub mod apu;
pub mod bus;
pub mod cart;
pub mod fs;
pub mod time;
#[macro_use]
pub mod common;
pub mod control_deck;
pub mod cpu;
pub mod error;
pub mod genie;
pub mod input;
pub mod mapper;
pub mod mem;
pub mod ppu;
pub mod sys;
pub mod video;