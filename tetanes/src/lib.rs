#![doc = include_str!("../../README.md")]
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

pub mod error;
pub mod logging;
pub mod nes;
pub mod platform;
pub mod sys;
pub mod thread;
