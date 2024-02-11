use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::{fs::File, io::BufReader};
use tetanes::control_deck::ControlDeck;
use web_time::Duration;

fn test_rom(frames: u32) {
    use std::path::PathBuf;

    let rom_path = PathBuf::from("roms/akumajou_densetsu.nes");
    assert!(rom_path.exists(), "No test rom found for {rom_path:?}");
    let mut rom = BufReader::new(File::open(&rom_path).expect("failed to open path"));
    let mut deck = ControlDeck::default();
    deck.load_rom(&rom_path.to_string_lossy(), &mut rom)
        .expect("failed to load rom");
    while deck.frame_number() < frames {
        deck.clock_frame().expect("valid frame clock");
        deck.clear_audio_samples();
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("nes");
    group.measurement_time(Duration::from_secs(60));
    group.bench_function("nes", |b| b.iter(|| test_rom(black_box(60))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
