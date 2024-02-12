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
#[derive(clap::Parser, Debug)]
#[command(version, author, about, long_about = None)]
#[must_use]
struct ConfigOpts {
    /// The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]
    path: Option<std::path::PathBuf>,
    /// A replay recording file for gameplay recording and playback.
    #[arg(short = 'e', long)]
    replay: Option<std::path::PathBuf>,
    /// Enable rewinding.
    #[arg(short, long, action=clap::ArgAction::SetTrue)]
    rewind: Option<bool>,
    /// Silence audio.
    #[arg(short, long, action=clap::ArgAction::SetTrue)]
    silent: Option<bool>,
    /// Start fullscreen.
    #[arg(short, long, action=clap::ArgAction::SetTrue)]
    fullscreen: Option<bool>,
    /// Disable VSync.
    #[arg(long, action=clap::ArgAction::SetTrue)]
    no_vsync: Option<bool>,
    /// Set four player adapter. [default: 'disabled']
    #[arg(short = 'p', long)]
    four_player: Option<tetanes::input::FourPlayer>,
    /// Enable zapper gun.
    #[arg(short, long, action=clap::ArgAction::SetTrue)]
    zapper: Option<bool>,
    /// Disable multi-threaded.
    #[arg(long, action=clap::ArgAction::SetTrue)]
    no_threaded: Option<bool>,
    /// Choose power-up RAM state. [default: "all_zeros"]
    #[arg(short = 'm', long)]
    ram_state: Option<tetanes::mem::RamState>,
    /// Save slot. [default: 1]
    #[arg(short = 'l', long)]
    save_slot: Option<u8>,
    /// Don't load save state on start.
    #[arg(short = 'i', long, action=clap::ArgAction::SetTrue)]
    no_load: Option<bool>,
    /// Window scale. [default: 3.0]
    #[arg(short = 'x', long)]
    scale: Option<f32>,
    /// Emulation speed. [default: 1.0]
    #[arg(long)]
    speed: Option<f32>,
    /// Add Game Genie Code(s).
    #[arg(short, long)]
    genie_code: Vec<String>,
    /// Start with debugger open.
    #[arg(short, long, action=clap::ArgAction::SetTrue)]
    debug: Option<bool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ConfigOpts {
    /// Extends a base `Config` with CLI options
    fn extend(base: Config) -> Config {
        use clap::Parser;

        let opts = Self::parse();
        log::debug!("CLI Options: {opts:?}");
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
            rewind: opts.rewind.unwrap_or(base.rewind),
            audio_enabled: !opts.silent.unwrap_or(base.audio_enabled),
            fullscreen: opts.fullscreen.unwrap_or(base.fullscreen),
            vsync: !opts.no_vsync.unwrap_or(base.vsync),
            four_player: opts.four_player.unwrap_or(base.four_player),
            zapper: opts.zapper.unwrap_or(base.zapper),
            threaded: !opts.no_threaded.unwrap_or(base.threaded),
            ram_state: opts.ram_state.unwrap_or(base.ram_state),
            save_slot: opts.save_slot.unwrap_or(base.save_slot),
            load_on_start: !opts.no_load.unwrap_or(base.load_on_start),
            scale: opts.scale.unwrap_or(base.scale),
            speed: opts.speed.unwrap_or(base.speed),
            debug: opts.debug.unwrap_or(base.debug),
            ..base
        };
        config.genie_codes.extend(opts.genie_code);
        config
    }
}
