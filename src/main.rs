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
    nes::{config::Config, Nes},
    NesResult,
};

#[cfg(target_arch = "wasm32")]
fn main() -> NesResult<()> {
    #[cfg(feature = "console_log")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        use log::Level;
        let log_level = if cfg!(debug_assertions) {
            Level::Debug
        } else {
            Level::Warn
        };
        console_log::init_with_level(log_level).expect("valid console log");
    }
    let config = Config::load();
    wasm_bindgen_futures::spawn_local(async {
        Nes::run(config).await.expect("Nes ran successfully")
    });
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> NesResult<()> {
    use std::{env, path::PathBuf};
    use structopt::StructOpt;

    #[derive(structopt::StructOpt, Debug)]
    #[must_use]
    #[structopt(
        name = "tetanes",
        about = "A NES Emulator written in Rust with WebAssembly support",
        version = "0.6.1",
        author = "Luke Petherbridge <me@lukeworks.tech>"
    )]
    /// `TetaNES` Command-Line Options
    struct Opt {
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

    if env::var("RUST_LOG").is_err() {
        let log_level = if cfg!(debug_assertions) {
            "tetanes=debug"
        } else {
            "tetanes=warn"
        };
        env::set_var("RUST_LOG", log_level);
    }

    pretty_env_logger::init_timed();
    #[cfg(feature = "profiling")]
    let _server = tetanes::profiling::start_server();

    let opt = Opt::from_args();
    let base_config = Config::load();
    let mut config = Config {
        rom_path: opt
            .path
            .map_or_else(
                || {
                    dirs::home_dir()
                        .or_else(|| env::current_dir().ok())
                        .unwrap_or_else(|| PathBuf::from("/"))
                },
                Into::into,
            )
            .canonicalize()?,
        replay_path: opt.replay,
        fullscreen: opt.fullscreen || base_config.fullscreen,
        ram_state: opt.ram_state.unwrap_or(base_config.ram_state),
        scale: opt.scale.unwrap_or(base_config.scale),
        speed: opt.speed.unwrap_or(base_config.speed),
        debug: opt.debug,
        ..base_config
    };
    config.genie_codes.extend(opt.genie_codes);

    pollster::block_on(async { Nes::run(config).await.expect("Nes ran successfully") });

    Ok(())
}
