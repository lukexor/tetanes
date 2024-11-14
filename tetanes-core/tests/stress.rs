use std::{fs::File, path::Path};
use tetanes_core::{
    control_deck::{Config, ControlDeck},
    mem::RamState,
};

#[test]
fn stress() {
    let base_path = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rom_path = base_path.join("test_roms/spritecans.nes");
    assert!(rom_path.exists(), "No test rom found for {rom_path:?}");
    let mut rom = File::open(&rom_path).expect("failed to open path");
    let mut deck = ControlDeck::with_config(Config {
        ram_state: RamState::AllZeros,
        ..Default::default()
    });
    deck.load_rom(rom_path.to_string_lossy(), &mut rom)
        .expect("failed to load rom");
    let frames = 1000;
    while deck.frame_number() < frames {
        deck.clock_frame().expect("valid frame clock");
        deck.clear_audio_samples();
    }
}
