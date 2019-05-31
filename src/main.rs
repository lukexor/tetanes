//! Usage: rustynes [rom_file | rom_directory]
//!
//! 1. If a rom file is provided, that rom is loaded
//! 2. If a directory is provided, `.nes` files are searched for in that directory
//! 3. If no arguments are provided, the current directory is searched for rom files ending in
//!    `.nes`
//!
//! In the case of 2 and 3, if valid NES rom files are found, a menu screen is displayed to select
//! which rom to run. If there are any errors related to invalid files, directories, or
//! permissions, the program will print an error and exit.

use failure::Error;
use rustynes::ui::UiBuilder;
use std::path::PathBuf;
use structopt::StructOpt;

fn main() {
    let opt = Opt::from_args();
    let mut ui = UiBuilder::new()
        .path(opt.path)
        .debug(opt.debug)
        .fullscreen(opt.fullscreen)
        .sound(!opt.sound_off)
        .save_slot(opt.save_slot)
        .scale(opt.scale)
        .build()
        .unwrap_or_else(|e| err_exit(e));
    ui.run().unwrap_or_else(|e| err_exit(e));
}

fn err_exit(err: Error) -> ! {
    eprintln!("Err: {}", err.to_string());
    std::process::exit(1);
}

/// Command-Line Options
#[derive(StructOpt, Debug)]
#[structopt(
    name = "rustynes",
    about = "An NES emulator written in Rust.",
    version = "0.1.0",
    author = "Luke Petherbridge <me@lukeworks.tech>"
)]
struct Opt {
    #[structopt(short = "d", long = "debug", help = "Debug")]
    debug: bool,
    #[structopt(short = "f", long = "fullscreen", help = "Fullscreen")]
    fullscreen: bool,
    #[structopt(
        long = "save_slot",
        default_value = "1",
        help = "Use Save Slot # (Options: 1-4)"
    )]
    save_slot: u8,
    #[structopt(
        short = "s",
        long = "scale",
        default_value = "3",
        help = "Window scale"
    )]
    scale: usize,
    #[structopt(long = "sound_off", help = "Disable Sound")]
    sound_off: bool,
    #[structopt(
        parse(from_os_str),
        help = "The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]"
    )]
    path: Option<PathBuf>,
}
