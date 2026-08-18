//! Render a frame of a ROM to a PNG.
//!
//! Useful for eyeballing boards that have no snapshot coverage, and for titles a maintainer can't
//! easily play through.
//!
//! ```sh
//! cargo run -p tetanes-utils --bin screenshot -- rom.nes --frames 300 --out shot.png
//! ```

use anyhow::Context;
use clap::Parser;
use std::{fs::File, path::PathBuf};
use tetanes_core::{
    control_deck::{Config, ControlDeck},
    memory::RamState,
    ppu::size,
};

#[derive(Parser, Debug)]
#[command(about = "Render a frame of a ROM to a PNG")]
struct Opt {
    /// ROM to load.
    path: PathBuf,
    /// Frames to clock before capturing.
    #[arg(short, long, default_value_t = 300)]
    frames: u32,
    /// Where to write the PNG.
    #[arg(short, long, default_value = "screenshot.png")]
    out: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let mut deck = ControlDeck::with_config(
        Config::default()
            .with_ram_state(RamState::AllZeros)
            // Rendering a frame is not playing the game; leave the player's saves alone.
            .with_sram_dir(None),
    );
    let mut rom =
        File::open(&opt.path).with_context(|| format!("failed to open {:?}", opt.path))?;
    deck.load_rom(opt.path.to_string_lossy(), &mut rom)
        .with_context(|| format!("failed to load {:?}", opt.path))?;

    for _ in 0..opt.frames {
        // Default speed, so a call is a frame and the display-frame drain a frontend
        // needs is not.
        let _ = deck.clock_frame().context("failed to clock frame")?;
    }

    image::RgbaImage::from_raw(
        u32::from(size::WIDTH),
        u32::from(size::HEIGHT),
        deck.frame_buffer().to_vec(),
    )
    .context("invalid frame buffer")?
    .save(&opt.out)
    .with_context(|| format!("failed to write {:?}", opt.out))?;

    println!("wrote {:?} after {} frames", opt.out, opt.frames);
    Ok(())
}
