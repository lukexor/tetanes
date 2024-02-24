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
    logging,
    nes::{config::Config, Nes},
    platform, NesResult,
};

fn main() -> NesResult<()> {
    logging::init();

    #[cfg(target_arch = "wasm32")]
    let config = Config::load();
    #[cfg(not(target_arch = "wasm32"))]
    let config = {
        use clap::Parser;

        let opts = ConfigOpts::parse();
        log::debug!("CLI Options: {opts:?}");

        let config = Config::load(opts.config.clone());
        opts.extend(config)?
    };

    platform::thread::spawn(Nes::run(config))
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
    #[arg(short = '4', long, value_enum)]
    four_player: Option<tetanes::input::FourPlayer>,
    /// Enable zapper gun.
    #[arg(short, long)]
    zapper: bool,
    /// Disable multi-threaded.
    #[arg(long)]
    no_threaded: bool,
    /// Choose power-up RAM state. [default: "all-zeros"]
    #[arg(short = 'm', long, value_enum)]
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
    /// Window scale. [default: x3]
    #[arg(short = 'x', long, value_enum)]
    scale: Option<tetanes::nes::config::Scale>,
    /// Emulation speed. [default: x100]
    #[arg(short = 'e', long, value_enum)]
    speed: Option<tetanes::nes::config::Speed>,
    /// Add Game Genie Code(s). e.g. `AATOZE` (Start Super Mario Bros. with 9 lives).
    #[arg(short, long)]
    genie_code: Vec<String>,
    /// Custom Config path.
    config: Option<std::path::PathBuf>,
    /// "Default Config" (skip user config and previous save states)
    #[arg(short, long)]
    clean: bool,
    /// Start with debugger open.
    #[arg(short, long)]
    debug: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl ConfigOpts {
    /// Extends a base `Config` with CLI options
    fn extend(mut self, mut base: Config) -> NesResult<Config> {
        use tetanes::{control_deck, genie::GenieCode};

        if self.clean {
            base = Config::default();
            self.no_load = true;
            self.no_save = true;
        }

        let mut config = Config {
            deck: control_deck::Config {
                four_player: self.four_player.unwrap_or(base.deck.four_player),
                zapper: self.zapper || base.deck.zapper,
                ram_state: self.ram_state.unwrap_or(base.deck.ram_state),
                save_slot: self.save_slot.unwrap_or(base.deck.save_slot),
                load_on_start: !self.no_load && base.deck.load_on_start,
                save_on_exit: !self.no_save && base.deck.save_on_exit,
                ..base.deck
            },
            rom_path: self
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
            replay_path: self.replay,
            rewind: self.rewind || base.rewind,
            audio_enabled: !self.silent && base.audio_enabled,
            fullscreen: self.fullscreen || base.fullscreen,
            vsync: !self.no_vsync && base.vsync,
            threaded: !self.no_threaded && base.threaded,
            scale: self.scale.unwrap_or(base.scale),
            frame_speed: self.speed.unwrap_or(base.frame_speed),
            debug: self.debug || base.debug,
            ..base
        };
        config.deck.genie_codes.reserve(self.genie_code.len());
        for genie_code in self.genie_code.into_iter() {
            config.deck.genie_codes.push(GenieCode::new(genie_code)?);
        }
        Ok(config)
    }
}
