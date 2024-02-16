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
    #[arg(short = 'p', long)]
    replay: Option<std::path::PathBuf>,
    /// Enable rewinding.
    #[arg(short, long)]
    rewind: bool,
    /// Silence audio.
    #[arg(short, long)]
    silent: bool,
    /// Start fullscreen.
    #[arg(short, long)]
    fullscreen: bool,
    /// Disable VSync.
    #[arg(long)]
    no_vsync: bool,
    /// Set four player adapter. [default: 'disabled']
    #[arg(short = '4', long)]
    four_player: Option<tetanes::input::FourPlayer>,
    /// Enable zapper gun.
    #[arg(short, long)]
    zapper: bool,
    /// Disable multi-threaded.
    #[arg(long)]
    no_threaded: bool,
    /// Choose power-up RAM state. [default: "all_zeros"]
    #[arg(short = 'm', long)]
    ram_state: Option<tetanes::mem::RamState>,
    /// Save slot. [default: 1]
    #[arg(short = 'i', long)]
    save_slot: Option<u8>,
    /// Don't load save state on start.
    #[arg(long)]
    no_load: bool,
    /// Don't auto save state or save on exit.
    #[arg(long)]
    no_save: bool,
    /// Window scale. [default: 3.0]
    #[arg(short = 'x', long)]
    scale: Option<f32>,
    /// Emulation speed. [default: 1.0]
    #[arg(short = 'e', long)]
    speed: Option<f32>,
    /// Add Game Genie Code(s).
    #[arg(short, long)]
    genie_code: Vec<String>,
    /// "Default Config" (skip user config and save states)
    #[arg(short, long)]
    clean: bool,
    /// Start with debugger open.
    #[arg(short, long)]
    debug: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl ConfigOpts {
    /// Extends a base `Config` with CLI options
    fn extend(mut base: Config) -> Config {
        use clap::Parser;
        use tetanes::control_deck;

        let mut opts = Self::parse();
        log::debug!("CLI Options: {opts:?}");

        if opts.clean {
            base = Config::default();
            opts.no_load = true;
            opts.no_save = true;
        }

        let mut config = Config {
            control_deck: control_deck::Config {
                four_player: opts.four_player.unwrap_or(base.control_deck.four_player),
                zapper: opts.zapper || base.control_deck.zapper,
                ram_state: opts.ram_state.unwrap_or(base.control_deck.ram_state),
                save_slot: opts.save_slot.unwrap_or(base.control_deck.save_slot),
                load_on_start: !opts.no_load && base.control_deck.load_on_start,
                save_on_exit: !opts.no_save && base.control_deck.save_on_exit,
                ..base.control_deck
            },
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
            rewind: opts.rewind || base.rewind,
            audio_enabled: !opts.silent && base.audio_enabled,
            fullscreen: opts.fullscreen || base.fullscreen,
            vsync: !opts.no_vsync && base.vsync,
            threaded: !opts.no_threaded && base.threaded,
            scale: opts.scale.unwrap_or(base.scale),
            frame_speed: opts.speed.unwrap_or(base.frame_speed),
            debug: opts.debug || base.debug,
            ..base
        };
        config.control_deck.genie_codes.extend(opts.genie_code);
        config
    }
}
