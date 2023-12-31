#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use tetanes::nes::{config::Config, Nes};

const TEST_DIR: &str = "test_roms";

fn test_rom_sound<P: AsRef<Path>>(rom: P, _run_frames: i32, _expected_hash: u64) {
    let rom = rom.as_ref();
    let config = Config {
        rom_path: PathBuf::from(TEST_DIR).join(rom),
        ..Default::default()
    };
    // TODO: Run control_deck and test sound output
    pollster::block_on(async { Nes::run(config).await.expect("valid run") })
}

macro_rules! test_rom {
    ($dir:expr, { $( ($test:ident, $run_frames:expr, $hash:expr$(, $ignore:expr)?$(,)?) ),* $(,)? }) => {$(
        $(#[ignore = $ignore])?
        #[test]
        fn $test() {
            test_rom_sound(concat!($dir, "/", stringify!($test), ".nes"), $run_frames, $hash);
        }
    )*};
}

// Requires --test-threads=1
test_rom!("apu", {
    (apu_env, 0, 0, "no automated way to test sound output (yet)"),
    (dmc, 0, 0, "no automated way to test sound output (yet)"),
    (dmc_buffer_retained, 0, 0, "no automated way to test sound output (yet)"),
    (dmc_latency, 0, 0, "no automated way to test sound output (yet)"),
    (dmc_pitch, 0, 0, "no automated way to test sound output (yet)"),
    (dmc_status, 0, 0, "no automated way to test sound output (yet)"),
    (dmc_status_irq, 0, 0, "no automated way to test sound output (yet)"),
    (lin_ctr, 0, 0, "no automated way to test sound output (yet)"),
    (noise, 0, 0, "no automated way to test sound output (yet)"),
    (noise_pitch, 0, 0, "no automated way to test sound output (yet)"),
    (phase_reset, 0, 0, "no automated way to test sound output (yet)"),
    (square, 0, 0, "no automated way to test sound output (yet)"),
    (square_pitch, 0, 0, "no automated way to test sound output (yet)"),
    (sweep_cutoff, 0, 0, "no automated way to test sound output (yet)"),
    (sweep_sub, 0, 0, "no automated way to test sound output (yet)"),
    (triangle, 0, 0, "no automated way to test sound output (yet)"),
    (triangle_pitch, 0, 0, "no automated way to test sound output (yet)"),
    (volumes, 0, 0, "no automated way to test sound output (yet)"),
});
