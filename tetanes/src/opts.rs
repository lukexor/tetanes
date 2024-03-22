use crate::nes::config::{Config, FrameSpeed, Scale};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use tetanes_core::{control_deck, genie::GenieCode};

#[derive(Debug, Clone)]
pub(crate) struct FourPlayer(tetanes_core::input::FourPlayer);

impl ValueEnum for FourPlayer {
    fn value_variants<'a>() -> &'a [Self] {
        use tetanes_core::input::FourPlayer::*;
        &[Self(Disabled), Self(FourScore), Self(Satellite)]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(clap::builder::PossibleValue::new(self.0.as_str()))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RamState(tetanes_core::mem::RamState);

impl ValueEnum for RamState {
    fn value_variants<'a>() -> &'a [Self] {
        use tetanes_core::mem::RamState::*;
        &[Self(AllZeros), Self(AllOnes), Self(Random)]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(clap::builder::PossibleValue::new(self.0.as_str()))
    }
}

/// `TetaNES` CLI Config Options
#[derive(Parser, Debug)]
#[command(version, author, about, long_about = None)]
#[must_use]
pub struct Opts {
    /// The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]
    pub(crate) path: Option<PathBuf>,
    /// A replay recording file for gameplay recording and playback.
    #[arg(short = 'p', long)]
    pub(crate) replay: Option<PathBuf>,
    /// Enable rewinding.
    #[arg(short, long)]
    pub(crate) rewind: bool,
    /// Silence audio.
    #[arg(short, long)]
    pub(crate) silent: bool,
    /// Start fullscreen.
    #[arg(short, long)]
    pub(crate) fullscreen: bool,
    /// Disable VSync.
    #[arg(long)]
    pub(crate) no_vsync: bool,
    /// Set four player adapter. [default: 'disabled']
    #[arg(short = '4', long, value_enum)]
    pub(crate) four_player: Option<FourPlayer>,
    /// Enable zapper gun.
    #[arg(short, long)]
    pub(crate) zapper: bool,
    /// Disable multi-threaded.
    #[arg(long)]
    pub(crate) no_threaded: bool,
    /// Choose power-up RAM state. [default: "all-zeros"]
    #[arg(short = 'm', long, value_enum)]
    pub(crate) ram_state: Option<RamState>,
    /// Save slot. [default: 1]
    #[arg(short = 'i', long)]
    pub(crate) save_slot: Option<u8>,
    /// Don't load save state on start.
    #[arg(long)]
    pub(crate) no_load: bool,
    /// Don't auto save state or save on exit.
    #[arg(long)]
    pub(crate) no_save: bool,
    /// Window scale. [default: 3.0]
    #[arg(short = 'x', long, value_parser = Scale::from_str_f32)]
    pub(crate) scale: Option<f32>,
    /// Emulation speed. [default: 1.0]
    #[arg(short = 'e', long, value_parser = FrameSpeed::from_str_f32)]
    pub(crate) speed: Option<f32>,
    /// Add Game Genie Code(s). e.g. `AATOZE` (Start Super Mario Bros. with 9 lives).
    #[arg(short, long)]
    pub(crate) genie_code: Vec<String>,
    /// Custom Config path.
    pub(crate) config: Option<PathBuf>,
    /// "Default Config" (skip user config and previous save states)
    #[arg(short, long)]
    pub(crate) clean: bool,
    /// Start with debugger open.
    #[arg(short, long)]
    pub(crate) debug: bool,
}

impl Opts {
    /// Loads a base `Config`, merging with CLI options
    pub fn load(self) -> anyhow::Result<Config> {
        let rom_path = self
            .path
            .map_or_else(
                || {
                    dirs::home_dir()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or(PathBuf::from("."))
                },
                Into::into,
            )
            .canonicalize()?;

        let base = if self.clean {
            let default = Config::default();
            Config {
                deck: control_deck::Config {
                    load_on_start: false,
                    save_on_exit: false,
                    ..default.deck
                },
                ..default
            }
        } else {
            Config::load(self.config.clone())
        };
        let mut config = Config {
            deck: control_deck::Config {
                four_player: self
                    .four_player
                    .map(|fp| fp.0)
                    .unwrap_or(base.deck.four_player),
                zapper: self.zapper || base.deck.zapper,
                ram_state: self.ram_state.map(|rs| rs.0).unwrap_or(base.deck.ram_state),
                save_slot: self.save_slot.unwrap_or(base.deck.save_slot),
                load_on_start: !self.no_load && base.deck.load_on_start,
                save_on_exit: !self.no_save && base.deck.save_on_exit,
                ..base.deck
            },
            rom_path,
            replay_path: self.replay,
            rewind: self.rewind || base.rewind,
            audio_enabled: !self.silent && base.audio_enabled,
            fullscreen: self.fullscreen || base.fullscreen,
            vsync: !self.no_vsync && base.vsync,
            threaded: !self.no_threaded && base.threaded,
            scale: self.scale.map(Scale::try_from).unwrap_or(Ok(base.scale))?,
            frame_speed: self
                .speed
                .map(FrameSpeed::try_from)
                .unwrap_or(Ok(base.frame_speed))?,
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
