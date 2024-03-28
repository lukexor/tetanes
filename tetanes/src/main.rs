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

use tetanes::{logging, nes::Nes, thread};

fn main() -> anyhow::Result<()> {
    let _log = logging::init();
    #[cfg(feature = "profiling")]
    puffin::set_scopes_on(true);

    #[cfg(target_arch = "wasm32")]
    let (path, config) = {
        use tetanes::nes::config::Config;
        (None, Config::load(None))
    };
    #[cfg(not(target_arch = "wasm32"))]
    let (path, config) = {
        use clap::Parser;
        let opts = tetanes::opts::Opts::parse();
        tracing::debug!("CLI Options: {opts:?}");
        opts.load()?
    };

    thread::spawn(Nes::run(path, config));

    Ok(())
}
