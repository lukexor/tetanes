use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::{fs::File, io::BufReader};
use tetanes::control_deck::ControlDeck;
use web_time::Duration;

fn clock_frames(frames: u32) {
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

fn benchmark_clock_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("nes");
    let seconds = 5;
    let frames = 60;
    let samples = 10;
    group.measurement_time(Duration::from_secs(samples * seconds));
    group.sample_size(samples as usize);
    group.bench_function("clock_frame", |b| {
        b.iter(|| clock_frames(black_box((seconds * frames) as u32)))
    });
    group.finish();
}

criterion_group!(benches, benchmark_clock_frame);
criterion_main!(benches);
