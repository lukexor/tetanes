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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cfg_if::cfg_if;
use tetanes::{
    logging,
    nes::{Nes, config::Config},
};

#[cfg(not(target_arch = "wasm32"))]
mod opts;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        fn load_config() -> anyhow::Result<Config> {
            Ok(Config::load(None))
        }
    } else {
        fn load_config() -> anyhow::Result<Config> {
            use clap::Parser;

            let opts = opts::Opts::parse();
            tracing::debug!("CLI Options: {opts:?}");

            opts.load()
        }
    }
}

fn main() -> anyhow::Result<()> {
    // Initialize logging early and handle error immediately
    if let Err(err) = logging::init() {
        eprintln!("failed to initialize logging: {err:?}");
    }

    #[cfg(feature = "profiling")]
    puffin::set_scopes_on(true);

    // Avoid unnecessary block expression
    Nes::run(cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            tetanes::nes::config::Config::load(None)
        } else {
            use clap::Parser;
            let opts = opts::Opts::parse();
            tracing::debug!("CLI Options: {opts:?}");
            opts.load()?
        }
    })
}
