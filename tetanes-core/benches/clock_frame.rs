#![allow(clippy::expect_used, reason = "fine in a benchmark")]

#[cfg(not(target_arch = "wasm32"))]
use criterion::{Criterion, criterion_group, criterion_main};
#[cfg(not(target_arch = "wasm32"))]
use pprof::criterion::{Output, PProfProfiler};
#[cfg(not(target_arch = "wasm32"))]
use std::{fs::File, hint::black_box, path::Path, time::Duration};
#[cfg(not(target_arch = "wasm32"))]
use tetanes_core::{
    control_deck::{Config, ControlDeck},
    mem::RamState,
};

#[cfg(not(target_arch = "wasm32"))]
fn clock_frames(rom_path: impl AsRef<Path>, frames: u32) {
    let base_path = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rom_path = base_path.join(rom_path);
    assert!(rom_path.exists(), "No test rom found for {rom_path:?}");
    let mut rom = File::open(&rom_path).expect("failed to open path");
    let mut deck = ControlDeck::with_config(Config {
        ram_state: RamState::AllZeros,
        ..Default::default()
    });
    deck.load_rom(rom_path.to_string_lossy(), &mut rom)
        .expect("failed to load rom");
    while deck.frame_number() < frames {
        deck.clock_frame().expect("valid frame clock");
        deck.clear_audio_samples();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn basic(c: &mut Criterion) {
    let mut group = c.benchmark_group("nes");
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(10);
    group.bench_function("basic", |b| {
        b.iter(|| clock_frames("test_roms/ppu/_240pee.nes", black_box(200)))
    });
    group.finish();
}

#[cfg(not(target_arch = "wasm32"))]
fn stress(c: &mut Criterion) {
    let mut group = c.benchmark_group("nes");
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(10);
    group.bench_function("stress", |b| {
        b.iter(|| clock_frames("test_roms/spritecans.nes", black_box(1000)));
    });
    group.finish();
}

#[cfg(not(target_arch = "wasm32"))]
criterion_group!(
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = basic, stress
);
#[cfg(not(target_arch = "wasm32"))]
criterion_main!(benches);

#[cfg(target_arch = "wasm32")]
fn main() {}
