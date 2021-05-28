#![warn(
    future_incompatible,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    nonstandard_style,
    rust_2018_idioms,
    trivial_casts,
    trivial_numeric_casts,
    unused,
    variant_size_differences
)]

//! Usage: tetanes [rom_file | rom_directory]
//!
//! 1. If a rom file is provided, that rom is loaded
//! 2. If a directory is provided, `.nes` files are searched for in that directory
//! 3. If no arguments are provided, the current directory is searched for rom files ending in
//!    `.nes`
//!
//! In the case of 2 and 3, if valid NES rom files are found, a menu screen is displayed to select
//! which rom to run. If there are any errors related to invalid files, directories, or
//! permissions, the program will print an error and exit.

use std::{env, path::PathBuf};
use structopt::StructOpt;
use tetanes::{nes::NesBuilder, NesErr};

fn main() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::init();

    let opt = Opt::from_args();
    NesBuilder::new()
        .path(
            opt.path
                .unwrap_or_else(|| env::current_dir().unwrap_or_default()),
        )
        .fullscreen(opt.fullscreen)
        .scale(opt.scale)
        .build()
        .run()
        .unwrap_or_else(|e| error_exit(e));
}

fn error_exit(e: NesErr) -> ! {
    eprintln!("Error: {}", e);
    std::process::exit(1);
}

/// Command-Line Options
#[derive(StructOpt, Debug)]
#[structopt(
    name = "tetanes",
    about = "A NES Emulator written in Rust with SDL2 and WebAssembly support",
    version = "0.6.1",
    author = "Luke Petherbridge <me@lukeworks.tech>"
)]
struct Opt {
    #[structopt(
        help = "The NES ROM to load, a directory containing `.nes` ROM files, or a recording playback `.playback` file. [default: current directory]"
    )]
    path: Option<PathBuf>,
    #[structopt(short = "f", long = "fullscreen", help = "Start fullscreen.")]
    fullscreen: bool,
    #[structopt(
        short = "s",
        long = "scale",
        default_value = "3.0",
        help = "Window scale"
    )]
    scale: f32,
}
