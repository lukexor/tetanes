#![allow(clippy::expect_used, reason = "fine in a benchmark")]

use std::{
    fs::File,
    hint::black_box,
    path::{Path, PathBuf},
    time::Instant,
};
use tetanes_core::prelude::*;

fn main() {
    const FRAMES_TO_RUN: u32 = 400;
    const ITERATIONS: u32 = 30;

    let rom_path = std::env::args()
        .find(|arg| arg.ends_with(".nes"))
        .map(PathBuf::from)
        .map(|path| {
            if path.exists() {
                path
            } else {
                // The working directory of every benchmark is set to the root directory of
                // the package the benchmark belongs to.
                //
                // So if path is relative, it might be relative to the workspace not the package
                // the benchmark is in
                std::env::current_dir()
                    .expect("valid cwd")
                    .join("..")
                    .join(path)
            }
        })
        .unwrap_or_else(|| {
            let base_path = Path::new(env!("CARGO_MANIFEST_DIR"));
            base_path.join("test_roms/spritecans.nes")
        });
    let rom_path = rom_path.canonicalize().expect("valid rom path");
    let mut rom = File::open(&rom_path).expect("failed to open path");

    let mut deck = ControlDeck::with_config(Config {
        ram_state: RamState::AllZeros,
        ..Default::default()
    });
    deck.load_rom(rom_path.to_string_lossy(), &mut rom)
        .expect("failed to load rom");

    // Warmup
    for _ in 0..3 {
        deck.reset(ResetKind::Hard);
        while deck.frame_number() < FRAMES_TO_RUN {
            black_box(deck.clock_frame()).expect("valid frame clock");
            deck.clear_audio_samples();
        }
    }

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        deck.reset(ResetKind::Hard);
        while deck.frame_number() < FRAMES_TO_RUN {
            black_box(deck.clock_frame()).expect("valid frame clock");
            deck.clear_audio_samples();
        }
    }
    let elapsed = start.elapsed().as_secs_f64();

    let ms_per_frame = (elapsed / f64::from(FRAMES_TO_RUN * ITERATIONS)) * 1000.0;
    println!("=== RESULTS ===");
    println!("{elapsed:.2} s total");
    println!("{ms_per_frame:.3} ms/frame");
}
