#![doc = include_str!("../README.md")]
#![allow(clippy::bool_to_int_with_if)]
#![warn(
    anonymous_parameters,
    bare_trait_objects,
    clippy::branches_sharing_code,
    clippy::map_unwrap_or,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::needless_for_each,
    clippy::redundant_closure_for_method_calls,
    clippy::semicolon_if_nothing_returned,
    clippy::unreadable_literal,
    clippy::unwrap_used,
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

pub mod audio;
pub mod genie;

pub mod apu;
pub mod bus;
pub mod cart;
#[macro_use]
pub mod common;
pub mod control_deck;
pub mod cpu;
#[cfg(not(target_arch = "wasm32"))]
pub mod debugger;
pub mod input;
pub mod mapper;
pub mod mem;
#[cfg(not(target_arch = "wasm32"))]
pub mod nes;
pub mod ppu;
pub mod video;

pub type NesError = anyhow::Error;
pub type NesResult<T> = anyhow::Result<T, NesError>;
