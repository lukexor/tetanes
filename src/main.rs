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

use std::env;
use structopt::StructOpt;
use tetanes::nes::{Nes, NesConfig};

fn main() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::init();

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
        pause_in_bg: !opt.no_pause_in_bg,
        fullscreen: opt.fullscreen,
        vsync: !opt.vsync_off,
        sound_enabled: !opt.sound_off,
        record: opt.record && opt.replay.is_none(),
        replay: opt.replay,
        rewind_enabled: opt.rewind,
        save_enabled: !opt.savestates_off,
        clear_save: opt.clear_savestate,
        concurrent_dpad: opt.concurrent_dpad,
        save_slot: opt.save_slot,
        scale: opt.scale,
        speed: opt.speed,
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
    name = "tetanes",
    about = "A NES Emulator written in Rust with SDL2 and WebAssembly support",
    version = "0.6.1",
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
        long = "no-pause-in-bg",
        help = "Pause emulation while the window is not in focus."
    )]
    no_pause_in_bg: bool,
    #[structopt(short = "f", long = "fullscreen", help = "Start fullscreen.")]
    fullscreen: bool,
    #[structopt(long = "vsync-off", help = "Disable vsync.")]
    vsync_off: bool,
    #[structopt(long = "sound-off", help = "Disable sound.")]
    sound_off: bool,
    #[structopt(
        short = "r",
        long = "record",
        help = "Record gameplay to a file for later action replay."
    )]
    record: bool,
    #[structopt(
        short = "p",
        long = "replay",
        help = "Replay a saved action replay file."
    )]
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
        short = "c",
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
        help = "Increase/Decrease emulation speed. (Ranges from 0.1 to 4.0)"
    )]
    speed: f32,
    #[structopt(
        short = "g",
        long = "genie-codes",
        help = "List of Game Genie Codes (space separated)."
    )]
    genie_codes: Vec<String>,
}
