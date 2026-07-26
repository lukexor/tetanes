//! Dump a cart's CHR as PNG tile sheets.
//!
//! Two views, because they answer different questions:
//!
//! - `--tables` renders the two pattern tables *as the PPU currently sees them*, after clocking
//!   `--frames`. That is what the game is actually drawing from, so it shows whether a board is
//!   banking the way you expect.
//! - The default renders every 4 KiB window of raw CHR, ignoring banking entirely, which is how
//!   you find where a tile actually lives when the screen is wrong.
//!
//! ```sh
//! cargo run -p tetanes-utils --bin chrdump -- rom.nes --frames 900 --out /tmp/chr
//! cargo run -p tetanes-utils --bin chrdump -- rom.nes --tables --out /tmp/chr
//! ```

use anyhow::Context;
use clap::Parser;
use std::{fs::File, path::PathBuf};
use tetanes_core::{
    control_deck::{Config, ControlDeck},
    memory::{RamState, Src},
};

/// Tiles per row in a sheet. 16 gives the familiar 128x128 pattern-table layout.
const TILES_PER_ROW: u32 = 16;
/// Bytes per 2bpp 8x8 tile.
const TILE_SIZE: usize = 16;

#[derive(Parser, Debug)]
#[command(about = "Dump a cart's CHR as PNG tile sheets")]
struct Opt {
    /// ROM to load.
    path: PathBuf,
    /// Frames to clock before dumping, so banking reflects a real moment.
    #[arg(short, long, default_value_t = 300)]
    frames: u32,
    /// Dump the two pattern tables as the PPU currently sees them, rather than raw CHR.
    #[arg(short, long)]
    tables: bool,
    /// Output path prefix. Files are `<out>-<name>.png`.
    #[arg(short, long, default_value = "chr")]
    out: String,
}

/// Render 2bpp tile data as a greyscale sheet, `TILES_PER_ROW` tiles wide.
///
/// Greyscale rather than palettised on purpose: a tile has no palette until a nametable attribute
/// picks one, and colour indices are what you want to compare against the ROM.
fn sheet(data: &[u8]) -> image::GrayImage {
    let tiles = (data.len() / TILE_SIZE) as u32;
    let rows = tiles.div_ceil(TILES_PER_ROW);
    let mut img = image::GrayImage::new(TILES_PER_ROW * 8, rows * 8);
    for tile in 0..tiles {
        let (tx, ty) = ((tile % TILES_PER_ROW) * 8, (tile / TILES_PER_ROW) * 8);
        let base = tile as usize * TILE_SIZE;
        for row in 0..8 {
            let lo = data[base + row];
            let hi = data[base + row + 8];
            for col in 0..8u32 {
                let bit = 7 - col;
                let color = ((lo >> bit) & 1) | (((hi >> bit) & 1) << 1);
                // 0/85/170/255 keeps the four indices evenly and obviously distinct.
                img.put_pixel(tx + col, ty + row as u32, image::Luma([color * 85]));
            }
        }
    }
    img
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let mut deck = ControlDeck::with_config(Config {
        ram_state: RamState::AllZeros,
        ..Default::default()
    });
    let mut rom =
        File::open(&opt.path).with_context(|| format!("failed to open {:?}", opt.path))?;
    deck.load_rom(opt.path.to_string_lossy(), &mut rom)
        .with_context(|| format!("failed to load {:?}", opt.path))?;
    for _ in 0..opt.frames {
        deck.clock_frame().context("failed to clock frame")?;
        deck.clear_audio_samples();
    }

    let ppu = &deck.cpu().bus.ppu;
    if opt.tables {
        for (table, base) in [(0u16, 0x0000u16), (1, 0x1000)] {
            // Through `Ppu::chr_peek`, so this goes through the same page tables and read hooks
            // the emulation does rather than reaching into the board.
            let data: Vec<u8> = (0..0x1000).map(|i| ppu.chr_peek(base + i)).collect();
            let path = format!("{}-table{table}.png", opt.out);
            sheet(&data).save(&path)?;
            println!("wrote {path}");
        }
    } else {
        let chr = ppu.memory.region_ref(Src::Chr);
        println!("chr: {} bytes ({} 4K banks)", chr.len(), chr.len() / 0x1000);
        for (bank, data) in chr.chunks(0x1000).enumerate() {
            let path = format!("{}-4k{bank:02X}.png", opt.out);
            sheet(data).save(&path)?;
        }
        println!("wrote {}-4kXX.png", opt.out);
    }

    Ok(())
}
