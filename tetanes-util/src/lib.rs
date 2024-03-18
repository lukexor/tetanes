#![warn(
    clippy::all,
    future_incompatible,
    nonstandard_style,
    rust_2018_compatibility,
    rust_2018_idioms,
    rust_2021_compatibility,
    unused
)]

pub mod error;
pub mod filesystem;
pub mod platform;

pub use error::{Error as NesError, Result as NesResult};
