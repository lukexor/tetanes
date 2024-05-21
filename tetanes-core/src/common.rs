//! Common traits and constants.

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use thiserror::Error;

pub const SAVE_DIR: &str = "save";
pub const SRAM_DIR: &str = "sram";

#[derive(Error, Debug)]
#[must_use]
#[error("failed to parse `NesRegion`")]
pub struct ParseNesRegionError;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum NesRegion {
    Auto,
    #[default]
    Ntsc,
    Pal,
    Dendy,
}

impl NesRegion {
    pub const fn as_slice() -> &'static [Self] {
        &[
            NesRegion::Auto,
            NesRegion::Ntsc,
            NesRegion::Pal,
            NesRegion::Dendy,
        ]
    }

    #[must_use]
    pub fn is_auto(&self) -> bool {
        self == &Self::Auto
    }

    #[must_use]
    pub const fn is_ntsc(&self) -> bool {
        matches!(self, Self::Auto | Self::Ntsc)
    }

    #[must_use]
    pub fn is_pal(&self) -> bool {
        self == &Self::Pal
    }

    #[must_use]
    pub fn is_dendy(&self) -> bool {
        self == &Self::Dendy
    }

    #[must_use]
    pub fn aspect_ratio(&self) -> f32 {
        // https://www.nesdev.org/wiki/Overscan
        match self {
            Self::Auto | Self::Ntsc => 8.0 / 7.0,
            Self::Pal | Self::Dendy => 18.0 / 13.0,
        }
    }

    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Ntsc => "ntsc",
            Self::Pal => "pal",
            Self::Dendy => "dendy",
        }
    }
}

impl std::fmt::Display for NesRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Auto => "Auto",
            Self::Ntsc => "NTSC",
            Self::Pal => "PAL",
            Self::Dendy => "Dendy",
        };
        write!(f, "{s}")
    }
}

impl AsRef<str> for NesRegion {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for NesRegion {
    type Error = ParseNesRegionError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "auto" => Ok(Self::Auto),
            "ntsc" => Ok(Self::Ntsc),
            "pal" => Ok(Self::Pal),
            "dendy" => Ok(Self::Dendy),
            _ => Err(ParseNesRegionError),
        }
    }
}

impl TryFrom<usize> for NesRegion {
    type Error = ParseNesRegionError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Auto),
            1 => Ok(Self::Ntsc),
            2 => Ok(Self::Pal),
            3 => Ok(Self::Dendy),
            _ => Err(ParseNesRegionError),
        }
    }
}

/// Trait for types that have different behavior depending on NES region.
#[enum_dispatch(Mapper)]
pub trait Regional {
    fn region(&self) -> crate::common::NesRegion {
        crate::common::NesRegion::Ntsc
    }
    fn set_region(&mut self, _region: crate::common::NesRegion) {}
}

/// Type of reset for types that have different behavior for reset vs power cycling.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum ResetKind {
    Soft,
    Hard,
}

/// Trait for types that can can be reset.
#[enum_dispatch(Mapper)]
pub trait Reset {
    fn reset(&mut self, _kind: crate::common::ResetKind) {}
}

/// Trait for types that can clock a single cycle.
#[enum_dispatch(Mapper)]
pub trait Clock {
    fn clock(&mut self) -> usize {
        0
    }
}

/// Trait for types that can clock to a target cycle.
pub trait ClockTo {
    fn clock_to(&mut self, _cycle: usize) -> usize {
        0
    }
}

/// Trait for types that can output `f32` audio samples.
pub trait Sample {
    fn output(&self) -> f32;
}

/// Trait for types that can save RAM to disk.
#[enum_dispatch(Mapper)]
pub trait Sram {
    fn save(&self, _path: impl AsRef<std::path::Path>) -> crate::fs::Result<()> {
        Ok(())
    }

    fn load(&mut self, _path: impl AsRef<std::path::Path>) -> crate::fs::Result<()> {
        Ok(())
    }
}

/// Prints a hex dump of a given byte array starting at `addr_offset`.
#[must_use]
pub fn hexdump(data: &[u8], addr_offset: usize) -> Vec<String> {
    use std::cmp;

    let mut addr = 0;
    let len = data.len();
    let mut last_line_same = false;
    let mut output = Vec::new();

    let mut last_line = String::with_capacity(80);
    while addr <= len {
        let end = cmp::min(addr + 16, len);
        let line_data = &data[addr..end];
        let line_len = line_data.len();

        let mut line = String::with_capacity(80);
        for byte in line_data.iter() {
            let _ = write!(line, " {byte:02X}");
        }

        if line_len % 16 > 0 {
            let words_left = (16 - line_len) / 2;
            for _ in 0..3 * words_left {
                line.push(' ');
            }
        }

        if line_len > 0 {
            line.push_str("  |");
            for c in line_data {
                if (*c as char).is_ascii() && !(*c as char).is_control() {
                    let _ = write!(line, "{}", (*c as char));
                } else {
                    line.push('.');
                }
            }
            line.push('|');
        }
        if last_line == line {
            if !last_line_same {
                last_line_same = true;
                output.push("*".to_string());
            }
        } else {
            last_line_same = false;
            output.push(format!("{:08x} {}", addr + addr_offset, line));
        }
        last_line = line;

        addr += 16;
    }
    output
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::{
        action::Action,
        common::{Regional, Reset, ResetKind},
        control_deck::{Config, ControlDeck},
        input::Player,
        mem::RamState,
        ppu::Ppu,
        video::VideoFilter,
    };
    use anyhow::Context;
    use image::{ImageBuffer, Rgba};
    use serde::{Deserialize, Serialize};
    use std::{
        collections::hash_map::DefaultHasher,
        env,
        fmt::Write,
        fs::{self, File},
        hash::{Hash, Hasher},
        io::{BufReader, Read},
        path::{Path, PathBuf},
        sync::OnceLock,
    };
    use tracing::debug;

    pub(crate) const RESULT_DIR: &str = "test_results";

    static PASS_DIR: OnceLock<PathBuf> = OnceLock::new();
    static FAIL_DIR: OnceLock<PathBuf> = OnceLock::new();

    #[macro_export]
    macro_rules! test_roms {
        ($mod:ident, $directory:expr, $( $(#[ignore = $reason:expr])? $test:ident ),* $(,)?) => {
            mod $mod {$(
                $(#[ignore = $reason])?
                #[test]
                fn $test() -> anyhow::Result<()> {
                    $crate::common::tests::test_rom($directory, stringify!($test))
                }
            )*}
        };
    }

    // TODO: Instead of a bunch of optional fields, it should be an enum:
    // enum FrameAction {
    //   DeckAction(DeckAction),
    //   FrameHash(u64),
    //   AudioHash(u64),
    // }
    #[derive(Default, Debug, Clone, Serialize, Deserialize)]
    #[serde(default)]
    #[must_use]
    struct TestFrame {
        number: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hash: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<Action>,
        #[serde(skip_serializing)]
        audio: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[must_use]
    struct RomTest {
        name: String,
        #[serde(skip_serializing, default)]
        audio: bool,
        frames: Vec<TestFrame>,
    }

    fn get_rom_tests(directory: &str) -> anyhow::Result<(PathBuf, Vec<RomTest>)> {
        let file = PathBuf::from(directory)
            .join("tests")
            .with_extension("json");
        let mut content = String::with_capacity(1024);
        File::open(&file)
            .and_then(|mut file| file.read_to_string(&mut content))
            .with_context(|| format!("failed to read rom test data: {file:?}"))?;
        let tests = serde_json::from_str(&content)
            .with_context(|| format!("valid rom test data: {file:?}"))?;
        Ok((file, tests))
    }

    fn load_control_deck<P: AsRef<Path>>(path: P) -> ControlDeck {
        let path = path.as_ref();
        let mut rom = BufReader::new(File::open(path).expect("failed to open path"));
        let mut deck = ControlDeck::with_config(Config {
            ram_state: RamState::AllZeros,
            filter: VideoFilter::Pixellate,
            ..Default::default()
        });
        deck.load_rom(&path.to_string_lossy(), &mut rom)
            .expect("failed to load rom");
        deck
    }

    fn on_frame_action(test_frame: &TestFrame, deck: &mut ControlDeck) {
        if let Some(action) = test_frame.action {
            debug!("{:?}", action);
            match action {
                Action::Reset(kind) => deck.reset(kind),
                Action::MapperRevision(rev) => deck.set_mapper_revision(rev),
                Action::SetVideoFilter(filter) => deck.set_filter(filter),
                Action::SetNesRegion(format) => deck.set_region(format),
                Action::Joypad((player, button)) => {
                    let joypad = deck.joypad_mut(player);
                    joypad.set_button(button, true);
                }
                Action::ToggleZapperConnected => deck.connect_zapper(!deck.zapper_connected()),
                Action::ZapperAim((x, y)) => deck.aim_zapper(x, y),
                Action::ZapperTrigger => deck.trigger_zapper(),
                Action::LoadState
                | Action::SaveState
                | Action::SetSaveSlot(_)
                | Action::ToggleApuChannel(_)
                | Action::ZapperAimOffscreen
                | Action::FourPlayer(_) => (),
            }
        }
    }

    fn on_snapshot(
        test: &str,
        test_frame: &TestFrame,
        deck: &mut ControlDeck,
        count: usize,
    ) -> anyhow::Result<Option<(u64, u64, u32, PathBuf)>> {
        match test_frame.hash {
            Some(expected) => {
                let mut hasher = DefaultHasher::new();
                if test_frame.audio {
                    deck.audio_samples()
                        .iter()
                        .for_each(|s| s.to_le_bytes().hash(&mut hasher));
                } else {
                    deck.frame_buffer().hash(&mut hasher);
                }
                let actual = hasher.finish();
                debug!(
                    "frame: {}, matched: {}",
                    test_frame.number,
                    expected == actual
                );

                let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
                let result_dir = if env::var("UPDATE_SNAPSHOT").is_ok() || expected == actual {
                    PASS_DIR.get_or_init(|| {
                        let directory = base_dir.join(PathBuf::from(RESULT_DIR)).join("pass");
                        if let Err(err) = fs::create_dir_all(&directory) {
                            panic!("created pass test results dir: {directory:?}. {err}",);
                        }
                        directory
                    })
                } else {
                    FAIL_DIR.get_or_init(|| {
                        let directory = base_dir.join(PathBuf::from(RESULT_DIR)).join("fail");
                        if let Err(err) = fs::create_dir_all(&directory) {
                            panic!("created fail test results dir: {directory:?}. {err}",);
                        }
                        directory
                    })
                };
                let mut filename = test.to_owned();
                if let Some(ref name) = test_frame.name {
                    let _ = write!(filename, "_{name}");
                } else if count > 0 {
                    let _ = write!(filename, "_{}", count + 1);
                }
                let screenshot = result_dir
                    .join(PathBuf::from(filename))
                    .with_extension("png");

                ImageBuffer::<Rgba<u8>, &[u8]>::from_raw(
                    Ppu::WIDTH,
                    Ppu::HEIGHT,
                    deck.frame_buffer(),
                )
                .expect("valid frame")
                .save(&screenshot)
                .with_context(|| format!("failed to save screenshot: {screenshot:?}"))?;

                Ok(Some((expected, actual, test_frame.number, screenshot)))
            }
            None => Ok(None),
        }
    }

    pub(crate) fn test_rom(directory: &str, test_name: &str) -> anyhow::Result<()> {
        static INIT_TESTS: OnceLock<bool> = OnceLock::new();

        let base_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let initialized = INIT_TESTS.get_or_init(|| {
            use tracing_subscriber::{
                filter::Targets, fmt, layer::SubscriberExt, registry, util::SubscriberInitExt,
            };
            registry()
                .with(
                    env::var("RUST_LOG")
                        .ok()
                        .and_then(|filter| filter.parse::<Targets>().ok())
                        .unwrap_or_default(),
                )
                .with(
                    fmt::layer()
                        .compact()
                        .with_line_number(true)
                        .with_thread_ids(true)
                        .with_thread_names(true)
                        .with_writer(std::io::stderr),
                )
                .init();
            let result_dir = base_dir.join(PathBuf::from(RESULT_DIR));
            if result_dir.exists() {
                if let Err(err) = fs::remove_dir_all(&result_dir) {
                    panic!("failed to clear test results dir: {result_dir:?}. {err}",);
                }
            }
            true
        });
        if *initialized {
            debug!("Initialized tests");
        }

        let (test_file, mut tests) = get_rom_tests(directory)?;
        let mut test = tests.iter_mut().find(|test| test.name.eq(test_name));
        assert!(test.is_some(), "No test found matching {test_name:?}");
        let test = test.as_mut().expect("definitely has a test");

        let rom = base_dir
            .join(directory)
            .join(PathBuf::from(&test.name))
            .with_extension("nes");
        assert!(rom.exists(), "No test rom found for {rom:?}");

        let mut deck = load_control_deck(&rom);
        deck.cpu_mut().bus.apu.skip_mixing = !test.audio;

        let mut results = Vec::new();
        assert!(!test.frames.is_empty(), "No test frames found for {rom:?}");
        for test_frame in test.frames.iter() {
            debug!("{} - {:?}", test_frame.number, deck.joypad_mut(Player::One));

            while deck.frame_number() < test_frame.number {
                deck.clock_frame().expect("valid frame clock");
                if deck.frame_number() != test_frame.number && !test_frame.audio {
                    deck.clear_audio_samples();
                }
                deck.joypad_mut(Player::One).reset(ResetKind::Soft);
                deck.joypad_mut(Player::Two).reset(ResetKind::Soft);
            }

            on_frame_action(test_frame, &mut deck);
            if let Ok(Some(result)) = on_snapshot(&test.name, test_frame, &mut deck, results.len())
            {
                results.push(result);
            }
        }
        let mut update_required = false;
        for (mut expected, actual, frame_number, screenshot) in results {
            if env::var("UPDATE_SNAPSHOT").is_ok() && expected != actual {
                expected = actual;
                update_required = true;
                if let Some(frame) = &mut test
                    .frames
                    .iter_mut()
                    .find(|frame| frame.number == frame_number)
                {
                    frame.hash = Some(actual);
                }
            }
            assert!(
                expected == actual,
                "mismatched snapshot for {rom:?} -> {screenshot:?} (expected: {expected}, actual: {actual})",
            );
        }
        if update_required {
            File::create(&test_file)
                .context("failed to open rom test file")
                .and_then(|file| {
                    serde_json::to_writer_pretty(file, &tests)
                        .context("failed to serialize rom data")
                })
                .with_context(|| format!("failed to update snapshot: {test_file:?}"))?
        }

        Ok(())
    }

    test_roms!(
        cpu,
        "test_roms/cpu",
        branch_backward, // Tests branches jumping backward
        branch_basics,   // Tests branch instructions, including edge cases
        branch_forward,  // Tests branches jumping forward
        nestest,         // Tests all CPU instructions, including illegal opcodes
        // Verifies ram and registers are set/cleared correctly after reset
        ram_after_reset,
        regs_after_reset,
        // Tests CPU dummy reads
        dummy_reads,
        dummy_writes_oam,
        dummy_writes_ppumem,
        // Verifies cpu can execute code from any memory location, incl. I/O
        exec_space_apu,
        exec_space_ppuio,
        flag_concurrency,
        // Tests CPU several instruction combinations
        instr_abs,
        instr_abs_xy,
        instr_basics,
        instr_branches,
        instr_brk,
        instr_imm,
        instr_imp,
        instr_ind_x,
        instr_ind_y,
        instr_jmp_jsr,
        instr_misc,
        instr_rti,
        instr_rts,
        instr_special,
        instr_stack,
        instr_timing,
        instr_zp,
        instr_zp_xy,
        // Tests IRQ/NMI timings
        int_branch_delays_irq,
        int_cli_latency,
        int_irq_and_dma,
        int_nmi_and_brk,
        int_nmi_and_irq,
        overclock,
        // Tests cycle stealing behavior of DMC DMA while running sprite DMAs
        sprdma_and_dmc_dma,
        sprdma_and_dmc_dma_512,
        timing_test, // Tests CPU timing
    );
    test_roms!(
        ppu,
        "test_roms/ppu",
        _240pee,               // TODO: Run each test
        color,                 // TODO: Test all color combinations
        ntsc_torture,          // Tests PPU NTSC signal artifacts
        oam_read,              // Tests OAM reading ($2004)
        oam_stress,            // Stresses OAM ($2003) reads and writes ($2004)
        open_bus,              // Tests PPU open bus behavior
        palette,               // Tests simple scanline palette changes
        palette_ram,           // Tests palette RAM access
        read_buffer,           // Thoroughly tests PPU read buffer ($2007)
        scanline,              // Tests scanline rendering
        spr_hit_alignment,     // Tests sprite hit alignment
        spr_hit_basics,        // Tests sprite hit basics
        spr_hit_corners,       // Tests sprite hit corners
        spr_hit_double_height, // Tests sprite hit in x16 height mode
        spr_hit_edge_timing,   // Tests sprite hit edge timing
        spr_hit_flip,          // Tests sprite hit with sprite flip
        spr_hit_left_clip,     // Tests sprite hit with left edge clipped
        spr_hit_right_edge,    // Tests sprite hit right edge
        spr_hit_screen_bottom, // Tests sprite hit bottom
        spr_hit_timing_basics, // Tests sprite hit timing
        spr_hit_timing_order,  // Tests sprite hit order
        spr_overflow_basics,   // Tests sprite overflow basics
        spr_overflow_details,  // Tests more thorough sprite overflow
        spr_overflow_emulator,
        spr_overflow_obscure,    // Tests obscure sprite overflow cases
        spr_overflow_timing,     // Tests sprite overflow timing
        sprite_ram,              // Tests sprite ram
        tv,                      // Tests NTSC color and NTSC/PAL aspect ratio
        vbl_nmi_basics,          // Tests vblank NMI basics
        vbl_nmi_clear_timing,    // Tests vblank NMI clear timing
        vbl_nmi_control,         // Tests vblank NMI control
        vbl_nmi_disable,         // Tests vblank NMI disable
        vbl_nmi_even_odd_frames, // Tests vblank NMI on even/odd frames
        #[ignore = "clock is skipped too late relative to enabling BG Failed #3"]
        vbl_nmi_even_odd_timing, // Tests vblank NMI even/odd frame timing
        vbl_nmi_frame_basics,    // Tests vblank NMI frame basics
        vbl_nmi_off_timing,      // Tests vblank NMI off timing
        vbl_nmi_on_timing,       // Tests vblank NMI on timing
        vbl_nmi_set_time,        // Tests vblank NMI set timing
        vbl_nmi_suppression,     // Tests vblank NMI supression
        vbl_nmi_timing,          // Tests vblank NMI timing
        vbl_timing,              // Tests vblank timing
        vram_access,             // Tests video RAM access
    );
    test_roms!(
        apu,
        "test_roms/apu",
        // DMC DMA during $2007 read causes 2-3 extra $2007
        // reads before real read.
        //
        // Number of extra reads depends in CPU-PPU
        // synchronization at reset.
        dmc_dma_2007_read,
        // DMC DMA during $2007 write has no effect.
        // Output:
        // 22 11 22 AA 44 55 66 77
        // 22 11 22 AA 44 55 66 77
        // 22 11 22 AA 44 55 66 77
        // 22 11 22 AA 44 55 66 77
        // 22 11 22 AA 44 55 66 77
        dmc_dma_2007_write,
        //  DMC DMA during $4016 read causes extra $4016
        // read.
        // Output:
        // 08 08 07 08 08
        dmc_dma_4016_read,
        // Double read of $2007 sometimes ignores extra
        //  read, and puts odd things into buffer.
        //
        // Output (depends on CPU-PPU synchronization):
        // 22 33 44 55 66
        // 22 44 55 66 77 or
        // 22 33 44 55 66 or
        // 02 44 55 66 77 or
        // 32 44 55 66 77 or
        // 85CFD627 or F018C287 or 440EF923 or E52F41A5
        dmc_dma_double_2007_read,
        // Read of $2007 just before write behaves normally.
        //
        // Output:
        // 33 11 22 33 09 55 66 77
        // 33 11 22 33 09 55 66 77
        dmc_dma_read_write_2007,
        // This NES program demonstrates abusing the NTSC NES's sampled sound
        // playback hardware as a scanline timer to split the screen twice
        // without needing to use a mapper-generated IRQ.
        dpcmletterbox,
        // Blargg's APU tests
        //
        // Misc
        // ----
        // - The frame IRQ flag is cleared only when $4015 is read or $4017 is
        // written with bit 6 set ($40 or $c0).

        // - The IRQ handler is invoked at minimum 29833 clocks after writing $00
        // to $4017 (assuming the frame IRQ flag isn't already set, and nothing
        // else generates an IRQ during that time).

        // - After reset or power-up, APU acts as if $4017 were written with $00
        // from 9 to 12 clocks before first instruction begins. It is as if this
        // occurs (this generates a 10 clock delay):

        //       lda   #$00
        //       sta   $4017       ; 1
        //       lda   <0          ; 9 delay
        //       nop
        //       nop
        //       nop
        // reset:
        //       ...

        // - As shown, the frame irq flag is set three times in a row. Thus when
        // polling it, always read $4015 an extra time after the flag is found to
        // be set, to be sure it's clear afterwards,

        // wait: bit   $4015       ; V flag reflects frame IRQ flag
        //       bvc   wait
        //       bit   $4015       ; be sure irq flag is clear

        // or better yet, clear it before polling it:

        //       bit   $4015       ; clear flag first
        // wait: bit   $4015       ; V flag reflects frame IRQ flag
        //       bvc   wait
        //
        // See:
        // <https://github.com/christopherpow/nes-test-roms/tree/master/blargg_apu_2005.07.30>
        //
        // Tests basic length counter operation
        // 1) Passed tests
        // 2) Problem with length counter load or $4015
        // 3) Problem with length table, timing, or $4015
        // 4) Writing $80 to $4017 should clock length immediately
        // 5) Writing $00 to $4017 shouldn't clock length immediately
        // 6) Clearing enable bit in $4015 should clear length counter
        // 7) When disabled via $4015, length shouldn't allow reloading
        // 8) Halt bit should suspend length clocking
        len_ctr,
        // Tests all length table entries.
        // 1) Passed
        // 2) Failed. Prints four bytes $II $ee $cc $02 that indicate the length
        // load value written (ll), the value that the emulator uses ($ee), and the
        // correct value ($cc).
        len_table,
        // Tests basic operation of frame irq flag.
        // 1) Tests passed
        // 2) Flag shouldn't be set in $4017 mode $40
        // 3) Flag shouldn't be set in $4017 mode $80
        // 4) Flag should be set in $4017 mode $00
        // 5) Reading flag clears it
        // 6) Writing $00 or $80 to $4017 doesn't affect flag
        // 7) Writing $40 or $c0 to $4017 clears flag
        irq_flag,
        // Clock Jitter
        // ------------
        // Changes to the mode by writing to $4017 only occur on *even* internal
        // APU clocks; if written on an odd clock, the first step of the mode is
        // delayed by one clock. At power-up and reset, the APU is randomly in an
        // odd or even cycle with respect to the first clock of the first
        // instruction executed by the CPU.

        // ; assume even APU and CPU clocks occur together
        // lda   #$00
        // sta   $4017       ; mode begins in one clock
        // sta   <0          ; delay 3 clocks
        // sta   $4017       ; mode begins immediately
        //
        // Tests for APU clock jitter. Also tests basic timing of frame irq flag
        // since it's needed to determine jitter.
        // 1) Passed tests
        // 2) Frame irq is set too soon
        // 3) Frame irq is set too late
        // 4) Even jitter not handled properly
        // 5) Odd jitter not handled properly
        clock_jitter,
        // Mode 0 Timing
        // -------------
        // -5    lda   #$00
        // -3    sta   $4017
        // 0     (write occurs here)
        // 1
        // 2
        // 3
        // ...
        //       Step 1
        // 7459  Clock linear
        // ...
        //       Step 2
        // 14915 Clock linear & length
        // ...
        //       Step 3
        // 22373 Clock linear
        // ...
        //       Step 4
        // 29830 Set frame irq
        // 29831 Clock linear & length and set frame irq
        // 29832 Set frame irq
        // ...
        //       Step 1
        // 37289 Clock linear
        // ...
        // etc.
        //
        // Return current jitter in A. Takes an even number of clocks. Tests length
        // counter timing in mode 0.
        // 1) Passed tests
        // 2) First length is clocked too soon
        // 3) First length is clocked too late
        // 4) Second length is clocked too soon
        // 5) Second length is clocked too late
        // 6) Third length is clocked too soon
        // 7) Third length is clocked too late
        len_timing_mode0,
        // Mode 1 Timing
        // -------------
        // -5    lda   #$80
        // -3    sta   $4017
        // 0     (write occurs here)
        //       Step 0
        // 1     Clock linear & length
        // 2
        // ...
        //       Step 1
        // 7459  Clock linear
        // ...
        //       Step 2
        // 14915 Clock linear & length
        // ...
        //       Step 3
        // 22373 Clock linear
        // ...
        //       Step 4
        // 29829 (do nothing)
        // ...
        //       Step 0
        // 37283 Clock linear & length
        // ...
        // etc.
        //
        // Tests length counter timing in mode 1.
        // 1) Passed tests
        // 2) First length is clocked too soon
        // 3) First length is clocked too late
        // 4) Second length is clocked too soon
        // 5) Second length is clocked too late
        // 6) Third length is clocked too soon
        // 7) Third length is clocked too late
        len_timing_mode1,
        // Frame interrupt flag is set three times in a row 29831 clocks after
        // writing $4017 with $00.
        // 1) Success
        // 2) Flag first set too soon
        // 3) Flag first set too late
        // 4) Flag last set too soon
        // 5) Flag last set too late
        irq_flag_timing,
        // IRQ handler is invoked at minimum 29833 clocks after writing $00 to
        // $4017.
        // 1) Passed tests
        // 2) Too soon
        // 3) Too late
        // 4) Never occurred
        irq_timing,
        // After reset or power-up, APU acts as if $4017 were written with $00 from
        // 9 to 12 clocks before first instruction begins.
        // 1) Success
        // 2) $4015 didn't read back as $00 at power-up
        // 3) Fourth step occurs too soon
        // 4) Fourth step occurs too late
        reset_timing,
        // Changes to length counter halt occur after clocking length, not before.
        // 1) Passed tests
        // 2) Length shouldn't be clocked when halted at 14914
        // 3) Length should be clocked when halted at 14915
        // 4) Length should be clocked when unhalted at 14914
        // 5) Length shouldn't be clocked when unhalted at 14915
        len_halt_timing,
        // Write to length counter reload should be ignored when made during length
        // counter clocking and the length counter is not zero.
        // 1) Passed tests
        // 2) Reload just before length clock should work normally
        // 3) Reload just after length clock should work normally
        // 4) Reload during length clock when ctr = 0 should work normally
        // 5) Reload during length clock when ctr > 0 should be ignored
        len_reload_timing,
        // Verifies timing of length counter clocks in both modes
        // 2) First length of mode 0 is too soon
        // 3) First length of mode 0 is too late
        // 4) Second length of mode 0 is too soon
        // 5) Second length of mode 0 is too late
        // 6) Third length of mode 0 is too soon
        // 7) Third length of mode 0 is too late
        // 8) First length of mode 1 is too soon
        // 9) First length of mode 1 is too late
        // 10) Second length of mode 1 is too soon
        // 11) Second length of mode 1 is too late
        // 12) Third length of mode 1 is too soon
        // 13) Third length of mode 1 is too late
        len_timing,
        // Verifies basic DMC operation
        // 2) DMC isn't working well enough to test further
        // 3) Starting DMC should reload length from $4013
        // 4) Writing $10 to $4015 should restart DMC if previous sample finished
        // 5) Writing $10 to $4015 should not affect DMC if previous sample is
        // still playing
        // 6) Writing $00 to $4015 should stop current sample
        // 7) Changing $4013 shouldn't affect current sample length
        // 8) Shouldn't set DMC IRQ flag when flag is disabled
        // 9) Should set IRQ flag when enabled and sample ends
        // 10) Reading IRQ flag shouldn't clear it
        // 11) Writing to $4015 should clear IRQ flag
        // 12) Disabling IRQ flag should clear it
        // 13) Looped sample shouldn't end until $00 is written to $4015
        // 14) Looped sample shouldn't ever set IRQ flag
        // 15) Clearing loop flag and then setting again shouldn't stop loop
        // 16) Clearing loop flag should end sample once it reaches end
        // 17) Looped sample should reload length from $4013 each time it reaches
        // end
        // 18) $4013=0 should give 1-byte sample
        // 19) There should be a one-byte buffer that's filled immediately if empty
        dmc_basics,
        // Verifies the DMC's 16 rates
        dmc_rates,
        // Reset
        // See: <https://github.com/christopherpow/nes-test-roms/tree/master/apu_reset>
        //
        // At power and reset, $4015 is cleared.
        // 2) At power, $4015 should be cleared
        // 3) At reset, $4015 should be cleared
        reset_4015_cleared,
        // At power, it is as if $00 were written to $4017,
        // then a 9-12 clock delay, then execution from address
        // in reset vector.

        // At reset, same as above, except last value written
        // to $4017 is written again, rather than $00.

        // The delay from when $00 was written to $4017 is
        // printed. Delay after NES being powered off for a
        // minute is usually 9.

        // 2) Frame IRQ flag should be set later after power/reset
        // 3) Frame IRQ flag should be set sooner after power/reset
        reset_4017_timing,
        // At power, $4017 = $00.
        // At reset, $4017 mode is unchanged, but IRQ inhibit
        // flag is sometimes cleared.

        // 2) At power, $4017 should be written with $00
        // 3) At reset, $4017 should should be rewritten with last value written
        reset_4017_written,
        // At power and reset, IRQ flag is clear.

        // 2) At power, flag should be clear
        // 3) At reset, flag should be clear
        reset_irq_flag_cleared,
        // At power and reset, length counters are enabled.

        // 2) At power, length counters should be enabled
        // 3) At reset, length counters should be enabled, triangle unaffected
        reset_len_ctrs_enabled,
        // At power and reset, $4017, $4015, and length counters work
        // immediately.

        // 2) At power, writes should work immediately
        // 3) At reset, writes should work immediately
        reset_works_immediately,
        // 11 tests that verify a number of behaviors with the APU (including the frame counter)
        //
        // See: <https://forums.nesdev.org/viewtopic.php?f=3&t=11174>
        test_1,
        test_2,
        test_3,
        test_4,
        test_5,
        test_6,
        test_7,
        test_8,
        test_9,
        test_10,
        // PAL APU tests
        //
        // See: <https://github.com/christopherpow/nes-test-roms/tree/master/pal_apu_tests>
        //
        // Tests basic length counter operation
        // 1) Passed tests
        // 2) Problem with length counter load or $4015
        // 3) Problem with length table, timing, or $4015
        // 4) Writing $80 to $4017 should clock length immediately
        // 5) Writing $00 to $4017 shouldn't clock length immediately
        // 6) Clearing enable bit in $4015 should clear length counter
        // 7) When disabled via $4015, length shouldn't allow reloading
        // 8) Halt bit should suspend length clocking
        pal_len_ctr,
        // Tests all length table entries.
        // 1) Passed
        // 2) Failed. Prints four bytes $II $ee $cc $02 that indicate the length load
        // value written (ll), the value that the emulator uses ($ee), and the correct
        // value ($cc).
        pal_len_table,
        // Tests basic operation of frame irq flag.
        // 1) Tests passed
        // 2) Flag shouldn't be set in $4017 mode $40
        // 3) Flag shouldn't be set in $4017 mode $80
        // 4) Flag should be set in $4017 mode $00
        // 5) Reading flag clears it
        // 6) Writing $00 or $80 to $4017 doesn't affect flag
        // 7) Writing $40 or $c0 to $4017 clears flag
        pal_irq_flag,
        // Tests for APU clock jitter. Also tests basic timing of frame irq flag since
        // it's needed to determine jitter. It's OK if you don't implement jitter, in
        // which case you'll get error #5, but you can still run later tests without
        // problem.
        // 1) Passed tests
        // 2) Frame irq is set too soon
        // 3) Frame irq is set too late
        // 4) Even jitter not handled properly
        // 5) Odd jitter not handled properly
        pal_clock_jitter,
        // Tests length counter timing in mode 0.
        // 1) Passed tests
        // 2) First length is clocked too soon
        // 3) First length is clocked too late
        // 4) Second length is clocked too soon
        // 5) Second length is clocked too late
        // 6) Third length is clocked too soon
        // 7) Third length is clocked too late
        pal_len_timing_mode0,
        // Tests length counter timing in mode 1.
        // 1) Passed tests
        // 2) First length is clocked too soon
        // 3) First length is clocked too late
        // 4) Second length is clocked too soon
        // 5) Second length is clocked too late
        // 6) Third length is clocked too soon
        // 7) Third length is clocked too late
        pal_len_timing_mode1,
        // Frame interrupt flag is set three times in a row 33255 clocks after writing
        // $4017 with $00.
        // 1) Success
        // 2) Flag first set too soon
        // 3) Flag first set too late
        // 4) Flag last set too soon
        // 5) Flag last set too late
        pal_irq_flag_timing,
        // IRQ handler is invoked at minimum 33257 clocks after writing $00 to $4017.
        // 1) Passed tests
        // 2) Too soon
        // 3) Too late
        // 4) Never occurred
        pal_irq_timing,
        // Changes to length counter halt occur after clocking length, not before.
        // 1) Passed tests
        // 2) Length shouldn't be clocked when halted at 16628
        // 3) Length should be clocked when halted at 16629
        // 4) Length should be clocked when unhalted at 16628
        // 5) Length shouldn't be clocked when unhalted at 16629
        pal_len_halt_timing,
        // Write to length counter reload should be ignored when made during length
        // counter clocking and the length counter is not zero.
        // 1) Passed tests
        // 2) Reload just before length clock should work normally
        // 3) Reload just after length clock should work normally
        // 4) Reload during length clock when ctr = 0 should work normally
        // 5) Reload during length clock when ctr > 0 should be ignored
        pal_len_reload_timing,
        #[ignore = "todo: passes, compare output"]
        apu_env,
        #[ignore = "todo: passes, check status"]
        dmc_buffer_retained,
        #[ignore = "todo: passes, compare output"]
        dmc_latency,
        #[ignore = "todo: passes, compare output"]
        dmc_pitch,
        #[ignore = "todo: passes, check status"]
        dmc_status,
        #[ignore = "todo: passes, check status"]
        dmc_status_irq,
        #[ignore = "todo: passes, compare output"]
        lin_ctr,
        #[ignore = "todo: passes, compare output"]
        noise_pitch,
        // Tests pulse behavior when writing to $4003/$4007 (reset duty but not dividers)
        #[ignore = "todo: unknown, compare output"]
        phase_reset,
        #[ignore = "todo: passes, compare output"]
        square_pitch,
        #[ignore = "todo: passes, compare output"]
        sweep_cutoff,
        #[ignore = "todo: passes, compare output"]
        sweep_sub,
        #[ignore = "todo: passes, compare output"]
        triangle_pitch,
        // This program demonstrates the channel balance among implementations
        // of the NES architecture.

        // The pattern consists of a set of 12 tones, as close to 1000 Hz as
        // the NES allows:
        // 1. Channel 1, 1/8 duty
        // 2. Channel 1, 1/4 duty
        // 3. Channel 1, 1/2 duty
        // 4. Channel 1, 3/4 duty
        // 5. Channels 1 and 2, 1/8 duty
        // 6. Channels 1 and 2, 1/4 duty
        // 7. Channels 1 and 2, 1/2 duty
        // 8. Channels 1 and 2, 3/4 duty
        // 9. Channel 3
        // 10. Channel 4, long LFSR period
        // 11. Channel 4, short LFSR period
        // 12. Channel 5, amplitude 30

        // When the user presses A on controller 1, the pattern plays three
        // times, with channel 5 held steady at 0, 48, and 96.  The high point
        // of tone 12 each time is 30 units above the level for that time,
        // that is, 30, 78, and 126 respectively.
        //
        // See: <https://github.com/christopherpow/nes-test-roms/tree/master/volume_tests>
        #[ignore = "todo: unknown, compare output"]
        volumes,
        // Mixer
        // The test status is written to $6000. $80 means the test is running, $81
        // means the test needs the reset button pressed, but delayed by at least
        // 100 msec from now. $00-$7F means the test has completed and given that
        // result code.

        // To allow an emulator to know when one of these tests is running and the
        // data at $6000+ is valid, as opposed to some other NES program, $DE $B0
        // $G1 is written to $6001-$6003.
        //
        // A byte is reported as a series of tones. The code is in binary, with a
        // low tone for 0 and a high tone for 1, and with leading zeroes skipped.
        // The first tone is always a zero. A final code of 0 means passed, 1 means
        // failure, and 2 or higher indicates a specific reason. See the source
        // code of the test for more information about the meaning of a test code.
        // They are found after the set_test macro. For example, the cause of test
        // code 3 would be found in a line containing set_test 3. Examples:

        //  Tones         Binary  Decimal  Meaning
        //  - - - - - - - - - - - - - - - - - - - -
        //  low              0      0      passed
        //  low high        01      1      failed
        //  low high low   010      2      error 2
        //
        // See <https://github.com/christopherpow/nes-test-roms/tree/master/apu_mixer>
        #[ignore = "todo: passes, compare $6000 output"]
        dmc,
        #[ignore = "todo: passes, compare $6000 output"]
        noise,
        #[ignore = "todo: passes, compare $6000 output"]
        square,
        #[ignore = "todo: passes, compare $6000 output"]
        triangle,
    );
    test_roms!(
        input,
        "test_roms/input",
        zapper_flip,
        zapper_light,
        #[ignore = "todo"]
        zapper_stream,
        #[ignore = "todo"]
        zapper_trigger,
    );
    test_roms!(
        m004_txrom,
        "test_roms/mapper/m004_txrom",
        a12_clocking,
        clocking,
        details,
        rev_b,
        scanline_timing,
        big_chr_ram,
        rev_a,
    );
    test_roms!(m005_exram, "test_roms/mapper/m005_exrom", exram, basics);
}
