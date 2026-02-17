//! Frame-clocking benchmark.
//!
//! Measures `ControlDeck::clock_frame` across a corpus of ROMs, reporting per-ROM mean frame time
//! with standard deviation and coefficient of variation so that run-to-run noise is visible and
//! small regressions can be distinguished from measurement jitter.
//!
//! # ROM selection
//!
//! In precedence order:
//!
//! 1. Any `.nes` paths given as arguments.
//! 2. `TETANES_BENCH_ROMS` - either a directory (every `.nes` inside it, sorted) or a
//!    `:`-separated list of paths.
//! 3. `test_roms/spritecans.nes`, which is committed and always available.
//!
//! Because the committed test ROMs are small and synthetic, comparing against commercial ROMs
//! requires pointing at a local library:
//!
//! ```sh
//! cargo make bench -- ~/roms/"Super Mario Bros. 3 (USA).nes"
//! TETANES_BENCH_ROMS=~/roms cargo make bench
//! ```
//!
//! # Interpreting results
//!
//! Frames are clocked from a hard reset with no input injected, so most commercial ROMs are
//! measured on a title or attract screen rather than in gameplay. That is stable and repeatable,
//! which is what regression testing needs, but it under-reports a busy frame. `spritecans.nes` is
//! a sprite stress ROM and serves as the pessimistic end of the range.

#![allow(clippy::expect_used, reason = "fine in a benchmark")]

use std::{
    ffi::OsStr,
    fs::File,
    hint::black_box,
    path::{Path, PathBuf},
    time::Instant,
};
use tetanes_core::prelude::*;

/// Frames clocked per timed iteration.
const FRAMES_TO_RUN: u32 = 600;
/// Timed iterations per ROM. Reported statistics are across these.
const ITERATIONS: usize = 10;
/// Frames clocked before timing starts, to settle caches and get past boot.
const WARMUP_FRAMES: u32 = 120;

/// Timing results for a single ROM.
struct Report {
    name: String,
    mean_ms: f64,
    stddev_ms: f64,
    cv: f64,
    min_ms: f64,
    max_ms: f64,
}

fn main() {
    let roms = resolve_corpus();
    assert!(!roms.is_empty(), "no ROMs to benchmark");

    println!(
        "{ITERATIONS} iterations x {FRAMES_TO_RUN} frames ({WARMUP_FRAMES} warmup), {} ROM(s)\n",
        roms.len()
    );

    let reports = roms.iter().map(|rom| bench_rom(rom)).collect::<Vec<_>>();

    println!("\n=== RESULTS ===");
    println!(
        "{:<44} {:>10} {:>10} {:>8} {:>10} {:>10}",
        "rom", "ms/frame", "stddev", "cv", "min", "max"
    );
    for report in &reports {
        println!(
            "{:<44} {:>10.3} {:>10.4} {:>7.2}% {:>10.3} {:>10.3}",
            elide(&report.name, 44),
            report.mean_ms,
            report.stddev_ms,
            report.cv,
            report.min_ms,
            report.max_ms,
        );
    }

    if reports.len() > 1 {
        let total = reports.iter().map(|r| r.mean_ms).sum::<f64>();
        println!("\n{:<44} {:>10.3}", "geometric mean", geomean(&reports));
        println!(
            "{:<44} {:>10.3}",
            "arithmetic mean",
            total / reports.len() as f64
        );
    }
}

/// Benchmark a single ROM, printing per-iteration progress to stderr.
fn bench_rom(path: &Path) -> Report {
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    eprintln!("{name}");

    let mut samples = Vec::with_capacity(ITERATIONS);
    for iter in 0..ITERATIONS {
        // A fresh deck per iteration, rather than `reset`. `Reset for Bus` clears WRAM but not
        // mapper PRG-RAM/SRAM or mapper bank registers, so resetting would let battery-backed
        // saves and bank state carry over and each iteration would measure a different game
        // state. Loading costs a few ms against ~2s of timed frames.
        let mut deck = ControlDeck::with_config(Config {
            // Deterministic RAM so runs are comparable.
            ram_state: RamState::AllZeros,
            ..Default::default()
        });
        let mut rom = File::open(path).expect("failed to open rom");
        deck.load_rom(path.to_string_lossy(), &mut rom)
            .expect("failed to load rom");

        // Warmup is not timed: settles caches, branch predictors, and CPU frequency, and gets
        // past the ROM's boot sequence.
        run_frames(&mut deck, WARMUP_FRAMES);

        let start = Instant::now();
        run_frames(&mut deck, FRAMES_TO_RUN);
        let elapsed = start.elapsed().as_secs_f64();

        let ms_per_frame = (elapsed / f64::from(FRAMES_TO_RUN)) * 1000.0;
        samples.push(ms_per_frame);
        eprintln!("  iter {iter:>2}: {ms_per_frame:.3} ms/frame");
    }

    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let variance = samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / samples.len() as f64;
    let stddev = variance.sqrt();

    Report {
        name,
        mean_ms: mean,
        stddev_ms: stddev,
        cv: (stddev / mean) * 100.0,
        min_ms: samples.iter().copied().fold(f64::INFINITY, f64::min),
        max_ms: samples.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    }
}

fn run_frames(deck: &mut ControlDeck, frames: u32) {
    for _ in 0..frames {
        black_box(deck.clock_frame()).expect("valid frame clock");
        deck.clear_audio_samples();
    }
}

/// Resolve which ROMs to benchmark. See module docs for precedence.
fn resolve_corpus() -> Vec<PathBuf> {
    let args = std::env::args()
        .filter(|arg| arg.ends_with(".nes"))
        .map(PathBuf::from)
        .map(resolve_path)
        .collect::<Vec<_>>();
    if !args.is_empty() {
        return args;
    }

    if let Some(var) = std::env::var_os("TETANES_BENCH_ROMS") {
        let path = PathBuf::from(&var);
        if path.is_dir() {
            let mut roms = path
                .read_dir()
                .expect("failed to read TETANES_BENCH_ROMS directory")
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension() == Some(OsStr::new("nes")))
                .collect::<Vec<_>>();
            roms.sort();
            assert!(!roms.is_empty(), "no .nes files found in {path:?}");
            return roms;
        }
        return var
            .to_string_lossy()
            .split(':')
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .map(resolve_path)
            .collect();
    }

    vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("test_roms/spritecans.nes")]
}

/// Resolve a path, falling back to the workspace root for relative paths, since the working
/// directory of a benchmark is the package root rather than the workspace root.
fn resolve_path(path: PathBuf) -> PathBuf {
    if path.exists() {
        return path;
    }
    std::env::current_dir()
        .expect("valid cwd")
        .join("..")
        .join(&path)
        .canonicalize()
        .unwrap_or_else(|_| panic!("rom not found: {path:?}"))
}

fn geomean(reports: &[Report]) -> f64 {
    let sum_ln = reports.iter().map(|r| r.mean_ms.ln()).sum::<f64>();
    (sum_ln / reports.len() as f64).exp()
}

fn elide(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    format!("{}...", &s[..max.saturating_sub(3)])
}
