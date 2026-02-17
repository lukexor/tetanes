#![doc = include_str!("../README.md")]
#![doc(
    html_favicon_url = "https://github.com/lukexor/tetanes/blob/main/assets/linux/icon.png?raw=true",
    html_logo_url = "https://github.com/lukexor/tetanes/blob/main/assets/linux/icon.png?raw=true"
)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod logging;
pub mod nes;
#[cfg(not(target_arch = "wasm32"))]
pub mod opts;

pub(crate) mod platform;
pub(crate) mod sys;
pub(crate) mod thread;
