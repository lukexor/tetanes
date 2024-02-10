use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::{fs::File, io::BufReader};
use tetanes::control_deck::ControlDeck;

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
    if std::env::var("RUST_LOG").is_ok() {
        let _ = pretty_env_logger::try_init();
    }
    c.bench_function("nes", |b| b.iter(|| test_rom(black_box(5))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
