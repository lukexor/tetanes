#![doc = include_str!("../README.md")]
#![doc(
    html_favicon_url = "https://github.com/lukexor/tetanes/blob/main/assets/linux/icon.png?raw=true",
    html_logo_url = "https://github.com/lukexor/tetanes/blob/main/assets/linux/icon.png?raw=true"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
// The wasm `Send` check on the deferred viewport closure in `nes::renderer::gui::ppu_viewer`
// walks wgpu's web backend types past the default depth of 128.
#![recursion_limit = "256"]

pub mod error;
pub mod logging;
pub mod nes;
pub mod platform;
pub mod sys;
pub mod thread;
