#![doc = include_str!("../README.md")]
#![warn(
    anonymous_parameters,
    bare_trait_objects,
    deprecated_in_future,
    ellipsis_inclusive_range_patterns,
    future_incompatible,
    missing_copy_implementations,
    missing_debug_implementations,
    // missing_docs,
    nonstandard_style,
    rust_2018_compatibility,
    rust_2018_idioms,
    rust_2021_compatibility,
    rustdoc::bare_urls,
    rustdoc::broken_intra_doc_links,
    rustdoc::invalid_html_tags,
    rustdoc::invalid_rust_codeblocks,
    rustdoc::private_intra_doc_links,
    single_use_lifetimes,
    trivial_casts,
    trivial_numeric_casts,
    unreachable_pub,
    unused,
    variant_size_differences
)]
#![doc(
    html_favicon_url = "https://github.com/lukexor/tetanes/blob/main/static/tetanes_icon.png?raw=true",
    html_logo_url = "https://github.com/lukexor/tetanes/blob/main/static/tetanes_icon.png?raw=true"
)]

use pix_engine::prelude::*;
use pretty_env_logger as _;
use std::{fmt, result};
use structopt as _;

pub mod apu;
pub mod bus;
pub mod cartridge;
#[macro_use]
pub mod common;
pub mod control_deck;
pub mod cpu;
pub mod filter;
pub mod input;
pub mod mapper;
pub mod memory;
pub mod nes;
pub mod ppu;
pub mod serialization;

pub type NesResult<T> = result::Result<T, anyhow::Error>;

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
        }
        .into())
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description)
    }
}

impl fmt::Debug for NesErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

impl From<std::string::FromUtf8Error> for NesErr {
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self {
            description: err.to_string(),
        }
    }
}

impl From<NesErr> for PixError {
    fn from(err: NesErr) -> Self {
        Self::Other(err.into())
    }
}

impl From<anyhow::Error> for NesErr {
    fn from(err: anyhow::Error) -> Self {
        Self::new(&err.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
impl From<NesErr> for JsValue {
    fn from(err: NesErr) -> Self {
        JsValue::from_str(&err.to_string())
    }
}
