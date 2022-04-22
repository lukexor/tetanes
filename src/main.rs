//! A NES Emulator written in Rust with `SDL2` and `WebAssembly` support
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

#![windows_subsystem = "windows"]

use std::{env, path::PathBuf};
use structopt::StructOpt;
use tetanes::{memory::RamState, nes::NesBuilder, NesResult};

fn main() -> NesResult<()> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::init();

    let opt = Opt::from_args();
    NesBuilder::new()
        .path(opt.path)
        .replay(opt.replay)
        .fullscreen(opt.fullscreen)
        .ram_state(opt.ram_state)
        .scale(opt.scale)
        .speed(opt.speed)
        .genie_codes(opt.genie_codes)
        .debug(opt.debug)
        .build()?
        .run()
}

#[derive(StructOpt, Debug)]
#[must_use]
#[structopt(
    name = "tetanes",
    about = "A NES Emulator written in Rust with SDL2 and WebAssembly support",
    version = "0.6.1",
    author = "Luke Petherbridge <me@lukeworks.tech>"
)]
/// `TetaNES` Command-Line Options
struct Opt {
    #[structopt(
        help = "The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]"
    )]
    path: Option<PathBuf>,
    #[structopt(
        short = "r",
        long = "replay",
        help = "A `.replay` recording file for gameplay recording and playback."
    )]
    replay: Option<PathBuf>,
    #[structopt(short = "f", long = "fullscreen", help = "Start fullscreen.")]
    fullscreen: bool,
    #[structopt(
        long = "ram_state",
        help = "Choose power-up RAM state: 'all_zeros', `all_ones`, `random` (default)."
    )]
    ram_state: Option<RamState>,
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
    #[structopt(long = "debug", help = "Start debugging")]
    debug: bool,
}
