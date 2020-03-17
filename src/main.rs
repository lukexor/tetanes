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

use rustynes::{
    logging::LogLevel,
    nes::{Nes, NesConfig},
};
use std::env;
use structopt::StructOpt;

fn main() {
    let opt = Opt::from_args();
    let config = NesConfig {
        path: opt.path.unwrap_or_else(|| {
            if let Some(p) = env::current_dir().unwrap_or_default().to_str() {
                p.to_string()
            } else {
                String::new()
            }
        }),
        debug: opt.debug,
        log_level: match opt.log_level.as_ref() {
            "error" => LogLevel::Error,
            "warn" => LogLevel::Warn,
            "info" => LogLevel::Info,
            "debug" => LogLevel::Debug,
            "trace" => LogLevel::Trace,
            _ => LogLevel::Off,
        },
        fullscreen: opt.fullscreen,
        vsync: !opt.vsync_off && !opt.unlock_fps,
        sound_enabled: !opt.sound_off && !opt.unlock_fps,
        record: opt.record && opt.replay.is_none(),
        replay: opt.replay,
        rewind_enabled: opt.rewind,
        save_enabled: !opt.savestates_off,
        clear_save: opt.clear_savestate,
        concurrent_dpad: opt.concurrent_dpad,
        save_slot: opt.save_slot,
        scale: opt.scale,
        speed: opt.speed,
        unlock_fps: opt.unlock_fps,
        genie_codes: opt.genie_codes,
    };
    let nes = Nes::with_config(config).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });
    if let Err(e) = nes.run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Command-Line Options
#[derive(StructOpt, Debug)]
#[structopt(
    name = "rustynes",
    about = "A NES Emulator written in Rust with SDL2 and WebAssembly support",
    version = "0.5.0",
    author = "Luke Petherbridge <me@lukeworks.tech>"
)]
struct Opt {
    #[structopt(
        help = "The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]"
    )]
    path: Option<String>,
    #[structopt(
        short = "d",
        long = "debug",
        help = "Start with the CPU debugger enabled and emulation paused at first CPU instruction."
    )]
    debug: bool,
    #[structopt(
        short = "l",
        long = "log-level",
        default_value = "error",
        possible_values = &["off", "error", "warn", "info", "debug", "trace"],
        help = "Set logging level."
    )]
    log_level: String,
    #[structopt(short = "f", long = "fullscreen", help = "Start fullscreen.")]
    fullscreen: bool,
    #[structopt(short = "v", long = "vsync-off", help = "Disable vsync.")]
    vsync_off: bool,
    #[structopt(long = "sound-off", help = "Disable sound.")]
    sound_off: bool,
    #[structopt(
        long = "record",
        help = "Record gameplay to a file for later action replay."
    )]
    record: bool,
    #[structopt(long = "replay", help = "Replay a saved action replay file.")]
    replay: Option<String>,
    #[structopt(
        long = "concurrent-dpad",
        help = "Enables the ability to simulate concurrent L+R and U+D on the D-Pad."
    )]
    concurrent_dpad: bool,
    #[structopt(long = "rewind", help = "Enable savestate rewinding")]
    rewind: bool,
    #[structopt(long = "savestates-off", help = "Disable savestates")]
    savestates_off: bool,
    #[structopt(
        long = "clear-savestate",
        help = "Removes existing savestates for current save-slot"
    )]
    clear_savestate: bool,
    #[structopt(
        long = "savestate-slot",
        default_value = "1",
        possible_values = &["1", "2", "3", "4"],
        help = "Set savestate slot #."
    )]
    save_slot: u8,
    #[structopt(
        short = "s",
        long = "scale",
        default_value = "3",
        help = "Window scale"
    )]
    scale: u32,
    #[structopt(
        long = "speed",
        default_value = "1.0",
        help = "Increase/Decrease emulation speed."
    )]
    speed: f32,
    #[structopt(
        long = "unlock-fps",
        help = "Disables locking FPS to 60. Also disables sound."
    )]
    unlock_fps: bool,
    #[structopt(
        long = "genie-codes",
        help = "List of Game Genie Codes (space separated)."
    )]
    genie_codes: Vec<String>,
}
