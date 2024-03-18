//! A NES Emulator written in Rust with `WebAssembly` support
//!
//! USAGE:
//!     tetanes [FLAGS] [OPTIONS] [path]
//!
//! FLAGS:
//!     -f, --fullscreen    Start fullscreen.
//!     -h, --help          Prints help information
//!     -V, --version       Prints version information
//!
//! OPTIONS:
//!     -s, --scale <scale>    Window scale [default: 3.0]
//!
//! ARGS:
//!     <path>    The NES ROM to load, a directory containing `.nes` ROM files, or a recording
//!               playback `.playback` file. [default: current directory]

#![doc = include_str!("../../README.md")]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod logging;
pub mod nes;
#[cfg(not(target_arch = "wasm32"))]
pub mod opts;

use nes::Nes;
use tetanes_util::{platform, NesResult};

fn main() -> NesResult<()> {
    let _log_guard = logging::init();
    #[cfg(feature = "profiling")]
    puffin::set_scopes_on(true);

    #[cfg(target_arch = "wasm32")]
    let config = Config::load();
    #[cfg(not(target_arch = "wasm32"))]
    let config = {
        use clap::Parser;
        let opts = opts::Opts::parse();
        tracing::debug!("CLI Options: {opts:?}");
        opts.load()?
    };

    platform::thread::spawn(Nes::run(config))
}
