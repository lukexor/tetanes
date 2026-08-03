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
//!
//! By default this measures `ControlDeck::clock_frame` **plus** the `frame_buffer` read that
//! applies `Video::apply_filter`, since every real frame is filtered. Set
//! `TETANES_BENCH_NO_OUTPUT=1` to time the CPU/PPU/APU core alone - worth doing when A/B-ing a core
//! change, since the filter is a ~6% constant offset that dilutes the delta.
//!
//! **Baselines recorded in `README.md` before 2026-07 excluded the filter**, and so are comparable
//! to `TETANES_BENCH_NO_OUTPUT=1` runs, not to the current default.
//!
//! `TETANES_BENCH_NO_AUDIO=1` sets `HeadlessMode::NO_AUDIO`, and so `Apu::skip_mixing`, which
//! skips the mixing tables and the filter chain. It does **not** skip channel
//! clocking - the channels still tick, the DMC still steals CPU cycles, and emulation is
//! unchanged - so this isolates the cost of turning channel state into samples, not the cost of
//! the APU as a whole.
//!
//! `TETANES_BENCH_FILTER=pixellate` swaps the output path's NTSC filter for the plain palette
//! decode, which is what a low-power target would run. Default is NTSC, matching the shipped
//! default and every recorded baseline.
//!
//! `TETANES_BENCH_RUN_AHEAD=n` clocks with run-ahead enabled, which costs a console snapshot and
//! `n` extra frames per call. Off by default so the recorded baselines stay comparable.

#![allow(clippy::expect_used, reason = "fine in a benchmark")]

use std::{
    ffi::OsStr,
    fs::File,
    hint::black_box,
    path::{Path, PathBuf},
    time::Instant,
};
use tetanes_core::{prelude::*, video::VideoFilter};

/// Frames clocked per timed iteration. Override with `TETANES_BENCH_FRAMES`.
const FRAMES_TO_RUN: u32 = 600;
/// Timed iterations per ROM. Reported statistics are across these. Override with
/// `TETANES_BENCH_ITERS`.
const ITERATIONS: usize = 10;
/// Frames clocked before timing starts, to settle caches and get past boot. Override with
/// `TETANES_BENCH_WARMUP`.
const WARMUP_FRAMES: u32 = 120;

/// Read a `usize` override from the environment.
///
/// Lowering these turns the benchmark into a quick smoke sweep over a large ROM library, where a
/// board that maps its banks wrongly enough to derail the CPU shows up as a frame-clock error.
fn env_or(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Whether the timed loop reads `frame_buffer` after each `clock_frame`, and so also runs
/// `Video::apply_filter` (the NTSC/Pixellate filters).
///
/// On by default, because every real frame is filtered and leaving it out under-reports frame time
/// by ~6%. Set `TETANES_BENCH_NO_OUTPUT=1` to isolate the CPU/PPU/APU core, which is worth doing
/// when A/B-ing a core change: the filter is a constant offset that dilutes the delta.
fn bench_output() -> bool {
    std::env::var_os("TETANES_BENCH_NO_OUTPUT").is_none()
}

/// Whether to skip audio mixing (`Apu::skip_mixing`), leaving channel clocking intact.
///
/// Off by default. Useful for splitting the APU's frame-time share into "producing channel state"
/// versus "turning that state into samples", which are separate optimization targets.
fn bench_no_audio() -> bool {
    std::env::var_os("TETANES_BENCH_NO_AUDIO").is_some()
}

/// Video filter for the output path: `TETANES_BENCH_FILTER=pixellate|ntsc`, default NTSC.
///
/// The NTSC filter is the shipped default; Pixellate is the cheap palette decode, which is what a
/// low-power target would run. Only meaningful without `TETANES_BENCH_NO_OUTPUT`.
fn bench_filter() -> VideoFilter {
    match std::env::var("TETANES_BENCH_FILTER").as_deref() {
        Ok("pixellate") => VideoFilter::Pixellate,
        _ => VideoFilter::Ntsc,
    }
}

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

    let frames = env_or("TETANES_BENCH_FRAMES", FRAMES_TO_RUN as usize) as u32;
    let iterations = env_or("TETANES_BENCH_ITERS", ITERATIONS);
    let warmup = env_or("TETANES_BENCH_WARMUP", WARMUP_FRAMES as usize) as u32;
    let output = bench_output();
    let no_audio = bench_no_audio();
    let filter = bench_filter();
    let run_ahead = env_or("TETANES_BENCH_RUN_AHEAD", 0);

    println!(
        "{iterations} iterations x {frames} frames ({warmup} warmup), {} ROM(s){}{}{}\n",
        roms.len(),
        if output {
            match filter {
                VideoFilter::Pixellate => ", +frame_buffer (pixellate)",
                VideoFilter::Ntsc => ", +frame_buffer",
            }
        } else {
            ""
        },
        if no_audio { ", no audio mixing" } else { "" },
        if run_ahead > 0 {
            format!(", run_ahead {run_ahead}")
        } else {
            String::new()
        },
    );

    let mut reports = Vec::with_capacity(roms.len());
    let mut skipped = Vec::new();
    for rom in &roms {
        match bench_rom(
            rom, frames, iterations, warmup, output, no_audio, filter, run_ahead,
        ) {
            Ok(report) => reports.push(report),
            // Sweeping a whole library will turn up boards this emulator does not implement yet.
            // Report them rather than aborting the run.
            Err(err) => skipped.push((rom.clone(), err)),
        }
    }

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

    if !skipped.is_empty() {
        println!("\n=== SKIPPED ({}) ===", skipped.len());
        for (path, err) in &skipped {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            println!("{:<44} {err}", elide(&name, 44));
        }
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
fn bench_rom(
    path: &Path,
    frames: u32,
    iterations: usize,
    warmup: u32,
    output: bool,
    no_audio: bool,
    filter: VideoFilter,
    run_ahead: usize,
) -> Result<Report, String> {
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    eprintln!("{name}");

    let mut samples = Vec::with_capacity(iterations);
    for iter in 0..iterations {
        // A fresh deck per iteration, rather than `reset`. `Reset for Bus` clears WRAM but not
        // mapper PRG-RAM/SRAM or mapper bank registers, so resetting would let battery-backed
        // saves and bank state carry over and each iteration would measure a different game
        // state. Loading costs a few ms against ~2s of timed frames.
        let mut deck = ControlDeck::with_config(Config {
            // Deterministic RAM so runs are comparable.
            ram_state: RamState::AllZeros,
            filter,
            run_ahead,
            headless_mode: if no_audio {
                HeadlessMode::NO_AUDIO
            } else {
                HeadlessMode::empty()
            },
            ..Default::default()
        });
        let mut rom = File::open(path).map_err(|err| err.to_string())?;
        deck.load_rom(path.to_string_lossy(), &mut rom)
            .map_err(|err| err.to_string())?;

        // Warmup is not timed: settles caches, branch predictors, and CPU frequency, and gets
        // past the ROM's boot sequence.
        run_frames(&mut deck, warmup, output);

        let start = Instant::now();
        run_frames(&mut deck, frames, output);
        let elapsed = start.elapsed().as_secs_f64();

        let ms_per_frame = (elapsed / f64::from(frames)) * 1000.0;
        samples.push(ms_per_frame);
        eprintln!("  iter {iter:>2}: {ms_per_frame:.3} ms/frame");
    }

    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let variance = samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / samples.len() as f64;
    let stddev = variance.sqrt();

    Ok(Report {
        name,
        mean_ms: mean,
        stddev_ms: stddev,
        cv: (stddev / mean) * 100.0,
        min_ms: samples.iter().copied().fold(f64::INFINITY, f64::min),
        max_ms: samples.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    })
}

fn run_frames(deck: &mut ControlDeck, frames: u32, output: bool) {
    if output {
        for _ in 0..frames {
            let _ = black_box(deck.clock_frame()).expect("valid frame clock");
            // `clock_frame` leaves the frame unfiltered; reading it is what pulls in
            // `Video::apply_filter`, which is the cost this mode exists to measure.
            black_box(deck.frame_buffer().len());
        }
        return;
    }
    for _ in 0..frames {
        let _ = black_box(deck.clock_frame()).expect("valid frame clock");
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
