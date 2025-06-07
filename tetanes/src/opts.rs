use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use tetanes::nes::config::Config;
use tetanes_core::genie::GenieCode;

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

#[derive(Debug, Clone)]
pub(crate) struct NesRegion(tetanes_core::common::NesRegion);

impl ValueEnum for NesRegion {
    fn value_variants<'a>() -> &'a [Self] {
        use tetanes_core::common::NesRegion::*;
        &[Self(Ntsc), Self(Pal), Self(Dendy)]
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
    /// Enable rewinding.
    #[arg(long)]
    pub(crate) rewind: bool,
    /// Silence audio.
    #[arg(short, long)]
    pub(crate) silent: bool,
    /// Start fullscreen.
    #[arg(short, long)]
    pub(crate) fullscreen: bool,
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
    /// Whether to emulate PPU warmup where writes to certain registers are ignored. Can result in
    /// some games not working correctly.
    #[arg(short = 'w', long)]
    pub(crate) emulate_ppu_warmup: bool,
    /// Choose default NES region. [default: "ntsc"]
    #[arg(short = 'r', long, value_enum)]
    pub(crate) region: Option<NesRegion>,
    /// Save slot. [default: 1]
    #[arg(short = 'i', long)]
    pub(crate) save_slot: Option<u8>,
    /// Don't load save state on start.
    #[arg(long)]
    pub(crate) no_load: bool,
    /// Don't auto save state or save on exit.
    #[arg(long)]
    pub(crate) no_save: bool,
    #[arg(short = 'x', long)]
    /// Emulation speed. [default: 1.0]
    pub(crate) speed: Option<f32>,
    /// Add Game Genie Code(s). e.g. `AATOZE` (Start Super Mario Bros. with 9 lives).
    #[arg(short, long)]
    pub(crate) genie_code: Vec<String>,
    /// Custom Config path.
    #[arg(long)]
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
        let mut cfg = if self.clean {
            Config::default()
        } else {
            Config::load(self.config.clone())
        };

        if let Some(FourPlayer(four_player)) = self.four_player {
            cfg.deck.four_player = four_player;
        }
        cfg.deck.zapper = self.zapper || cfg.deck.zapper;
        if let Some(RamState(ram_state)) = self.ram_state {
            cfg.deck.ram_state = ram_state;
        }
        cfg.deck.emulate_ppu_warmup = self.emulate_ppu_warmup || cfg.deck.emulate_ppu_warmup;
        if let Some(NesRegion(region)) = self.region {
            cfg.deck.region = region;
        }
        cfg.deck.genie_codes.reserve(self.genie_code.len());
        for genie_code in self.genie_code.into_iter() {
            cfg.deck.genie_codes.push(GenieCode::new(genie_code)?);
        }

        cfg.emulation.auto_load = if self.clean {
            false
        } else {
            !self.no_load && cfg.emulation.auto_load
        };
        cfg.emulation.rewind = self.rewind || cfg.emulation.rewind;
        cfg.emulation.auto_save = if self.clean {
            false
        } else {
            !self.no_save && cfg.emulation.auto_save
        };
        if let Some(save_slot) = self.save_slot {
            cfg.emulation.save_slot = save_slot
        }
        if let Some(speed) = self.speed {
            cfg.emulation.speed = speed
        }
        cfg.emulation.threaded = !self.no_threaded && cfg.emulation.threaded;

        cfg.audio.enabled = !self.silent && cfg.audio.enabled;

        cfg.renderer.roms_path = self
            .path
            .or(cfg.renderer.roms_path)
            .and_then(|path| path.canonicalize().ok());
        cfg.renderer.fullscreen = self.fullscreen || cfg.renderer.fullscreen;

        Ok(cfg)
    }
}
