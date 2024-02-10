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

use tetanes::{
    nes::{self, config::Config, Nes},
    profiling, NesResult,
};

mod logging;

fn main() -> NesResult<()> {
    logging::init();
    profiling::init();

    let config = Config::load();
    #[cfg(not(target_arch = "wasm32"))]
    let config = ConfigOpts::extend(config);

    nes::platform::spawn(Nes::run(config))
}

/// `TetaNES` CLI Config Options
#[cfg(not(target_arch = "wasm32"))]
#[derive(structopt::StructOpt, Debug)]
#[must_use]
#[structopt(
    name = "tetanes",
    about = "A NES Emulator written in Rust with WebAssembly support",
    version = "0.6.1",
    author = "Luke Petherbridge <me@lukeworks.tech>"
)]
struct ConfigOpts {
    #[structopt(
        help = "The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]"
    )]
    path: Option<std::path::PathBuf>,
    #[structopt(
        short = "r",
        long = "replay",
        help = "A `.replay` recording file for gameplay recording and playback."
    )]
    replay: Option<std::path::PathBuf>,
    #[structopt(short = "f", long = "fullscreen", help = "Start fullscreen.")]
    fullscreen: bool,
    #[structopt(
        long = "ram_state",
        help = "Choose power-up RAM state: 'all_zeros' (default), `all_ones`, `random`."
    )]
    ram_state: Option<tetanes::mem::RamState>,
    #[structopt(short = "s", long = "scale", help = "Window scale, defaults to 3.0.")]
    scale: Option<f32>,
    #[structopt(long = "speed", help = "Emulation speed, defaults to 1.0.")]
    speed: Option<f32>,
    #[structopt(
        short = "g",
        long = "genie-codes",
        help = "List of Game Genie Codes (space separated)."
    )]
    genie_codes: Vec<String>,
    #[structopt(long = "debug", help = "Start with debugger")]
    debug: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl ConfigOpts {
    /// Extends a base `Config` with CLI options
    fn extend(base: Config) -> Config {
        use structopt::StructOpt;

        let opts = Self::from_args();
        let mut config = Config {
            rom_path: opts
                .path
                .map_or_else(
                    || {
                        dirs::home_dir()
                            .or_else(|| std::env::current_dir().ok())
                            .unwrap_or_else(|| base.rom_path.clone())
                    },
                    Into::into,
                )
                .canonicalize()
                .unwrap_or(base.rom_path),
            replay_path: opts.replay,
            fullscreen: opts.fullscreen || base.fullscreen,
            ram_state: opts.ram_state.unwrap_or(base.ram_state),
            scale: opts.scale.unwrap_or(base.scale),
            speed: opts.speed.unwrap_or(base.speed),
            debug: opts.debug,
            ..base
        };
        config.genie_codes.extend(opts.genie_codes);
        config
    }
}
