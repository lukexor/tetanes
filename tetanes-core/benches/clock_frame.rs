#![allow(clippy::expect_used, reason = "fine in a benchmark")]

use criterion::{Criterion, criterion_group, criterion_main};
use std::{fs::File, hint::black_box, path::Path, time::Duration};
use tetanes_core::prelude::*;

fn clock_frames(c: &mut Criterion) {
    const FRAMES_TO_RUN: u32 = 600;

    let rom_path = std::env::args()
        .find(|arg| arg.ends_with(".nes"))
        .map(|path| std::env::current_dir().expect("valid cwd").join(path))
        .unwrap_or_else(|| {
            let base_path = Path::new(env!("CARGO_MANIFEST_DIR"));
            base_path.join("test_roms/spritecans.nes")
        });
    let rom_path = rom_path.canonicalize().expect("valid rom path");
    assert!(rom_path.exists(), "No test rom found for {rom_path:?}");

    let mut rom = File::open(&rom_path).expect("failed to open path");
    let mut deck = ControlDeck::with_config(Config {
        ram_state: RamState::AllZeros,
        ..Default::default()
    });
    deck.load_rom(rom_path.to_string_lossy(), &mut rom)
        .expect("failed to load rom");

    let mut group = c.benchmark_group("nes");
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("clock_frame", |b| {
        deck.reset(ResetKind::Hard);
        b.iter(|| {
            while deck.frame_number() < FRAMES_TO_RUN {
                black_box(deck.clock_frame()).expect("valid frame clock");
                deck.clear_audio_samples();
            }
        });
    });
    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets = clock_frames
);
criterion_main!(benches);
