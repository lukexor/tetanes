use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::{fs::File, io::BufReader};
use tetanes::control_deck::{Config, ControlDeck};
use web_time::Duration;

fn clock_frames(frames: u32) {
    use std::path::PathBuf;

    let rom_path = PathBuf::from("roms/akumajou_densetsu.nes");
    assert!(rom_path.exists(), "No test rom found for {rom_path:?}");
    let mut rom = BufReader::new(File::open(&rom_path).expect("failed to open path"));
    let mut deck = ControlDeck::with_config(Config {
        load_on_start: false,
        save_on_exit: false,
        ..Default::default()
    });
    deck.load_rom(&rom_path.to_string_lossy(), &mut rom)
        .expect("failed to load rom");
    while deck.frame_number() < frames {
        deck.clock_frame().expect("valid frame clock");
        deck.clear_audio_samples();
    }
}

fn benchmark_clock_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("nes");
    let frames = 60;
    group.measurement_time(Duration::from_secs(40));
    group.bench_function("clock_frame", |b| {
        b.iter(|| clock_frames(black_box(frames)))
    });
    group.finish();
}

criterion_group!(benches, benchmark_clock_frame);
criterion_main!(benches);
