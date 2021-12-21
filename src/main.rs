//! A NES Emulator written in Rust with SDL2 and WebAssembly support
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

use std::{env, path::PathBuf};
use structopt::StructOpt;
use tetanes::{nes::NesBuilder, NesResult};

fn main() -> NesResult<()> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::init();

    let opt = Opt::from_args();
    NesBuilder::new()
        .path(opt.path)
        .fullscreen(opt.fullscreen)
        .consistent_ram(opt.consistent_ram)
        .scale(opt.scale)
        .speed(opt.speed)
        .genie_codes(opt.genie_codes)
        .build()?
        .run()
}

#[derive(StructOpt, Debug)]
#[structopt(
    name = "tetanes",
    about = "A NES Emulator written in Rust with SDL2 and WebAssembly support",
    version = "0.6.1",
    author = "Luke Petherbridge <me@lukeworks.tech>"
)]
/// TetaNES Command-Line Options
struct Opt {
    #[structopt(
        help = "The NES ROM to load, a directory containing `.nes` ROM files, or a recording playback `.playback` file. [default: current directory]"
    )]
    path: Option<PathBuf>,
    #[structopt(short = "f", long = "fullscreen", help = "Start fullscreen.")]
    fullscreen: bool,
    #[structopt(long = "consistent_ram", help = "Power up with consistent ram state.")]
    consistent_ram: bool,
    #[structopt(
        short = "s",
        long = "scale",
        default_value = "3.0",
        help = "Window scale."
    )]
    scale: f32,
    #[structopt(long = "speed", default_value = "1.0", help = "Emulation speed.")]
    speed: f32,
    #[structopt(
        short = "g",
        long = "genie-codes",
        help = "List of Game Genie Codes (space separated)."
    )]
    genie_codes: Vec<String>,
}
