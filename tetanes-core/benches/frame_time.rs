#![allow(clippy::expect_used, reason = "fine in a benchmark")]

use std::{fs::File, hint::black_box, path::Path, time::Instant};
use tetanes_core::prelude::*;

fn main() {
    const WARMUP_FRAMES: usize = 120;
    const FRAMES_TO_RUN: u32 = 60 * 20;
    const ITERATIONS: usize = 5;

    let rom_path = std::env::args()
        .find(|arg| arg.ends_with(".nes"))
        .map(|path| std::env::current_dir().expect("valid cwd").join(path))
        .unwrap_or_else(|| {
            let base_path = Path::new(env!("CARGO_MANIFEST_DIR"));
            base_path.join("test_roms/spritecans.nes")
        });
    let rom_path = rom_path.canonicalize().expect("valid rom path");
    assert!(rom_path.exists(), "No test rom found for {rom_path:?}");

    let mut results = Vec::with_capacity(ITERATIONS);
    for iter in 0..ITERATIONS {
        let mut rom = File::open(&rom_path).expect("failed to open path");
        let mut deck = ControlDeck::with_config(Config {
            ram_state: RamState::AllZeros,
            ..Default::default()
        });
        deck.load_rom(rom_path.to_string_lossy(), &mut rom)
            .expect("failed to load rom");

        for _ in 0..WARMUP_FRAMES {
            black_box(deck.clock_frame()).expect("valid clock");
            deck.clear_audio_samples();
        }

        let start = Instant::now();
        for _ in 0..FRAMES_TO_RUN {
            black_box(deck.clock_frame()).expect("valid clock");
            deck.clear_audio_samples();
        }
        let elapsed = start.elapsed().as_secs_f64();
        results.push(elapsed);
        let frame_time = (elapsed / f64::from(FRAMES_TO_RUN)) * 1000.0;
        let fps = f64::from(FRAMES_TO_RUN) / elapsed;
        eprintln!("  iter {iter}: {elapsed:.3?} ({frame_time:.3} ms/frame, {fps:.1} fps)");
    }

    let mean = results.iter().sum::<f64>() / results.len() as f64;
    let variance = results.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / results.len() as f64;
    let stddev = variance.sqrt();
    let cv = (stddev / mean) * 100.0;
    let mean_frame_time = (mean / f64::from(FRAMES_TO_RUN)) * 1000.0;
    let mean_fps = f64::from(FRAMES_TO_RUN) / mean;

    println!(
        "\n=== Summary ({ITERATIONS} iterations, {FRAMES_TO_RUN} frames, {WARMUP_FRAMES} warmup) ==="
    );
    println!(
        "  mean: {mean:.4}s  stddev: {stddev:.6}s  cv: {cv:.2}%  frame time: {mean_frame_time:.3} ms/frame, fps: {mean_fps:.1}"
    );
}
