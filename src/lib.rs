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

use pretty_env_logger as _;
use structopt as _;

pub mod apu;
pub mod bus;
pub mod cart;
#[macro_use]
pub mod common;
pub mod control_deck;
pub mod cpu;
pub mod input;
pub mod mapper;
pub mod memory;
pub mod nes;
pub mod ppu;
pub mod serialization;

pub type NesResult<T> = anyhow::Result<T, anyhow::Error>;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
impl From<NesErr> for JsValue {
    fn from(err: NesErr) -> Self {
        JsValue::from_str(&err.to_string())
    }
}
