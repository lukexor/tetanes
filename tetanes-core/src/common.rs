use anyhow::anyhow;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use tetanes_util::{NesError, NesResult};

pub const SAVE_DIR: &str = "save";
pub const SRAM_DIR: &str = "sram";

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum NesRegion {
    #[default]
    Ntsc,
    Pal,
    Dendy,
}

impl NesRegion {
    pub const fn as_slice() -> &'static [Self] {
        &[NesRegion::Ntsc, NesRegion::Pal, NesRegion::Dendy]
    }

    #[must_use]
    pub fn is_ntsc(&self) -> bool {
        self == &Self::Ntsc
    }

    #[must_use]
    pub fn is_pal(&self) -> bool {
        self == &Self::Pal
    }

    #[must_use]
    pub fn is_dendy(&self) -> bool {
        self == &Self::Dendy
    }
}

impl AsRef<str> for NesRegion {
    fn as_ref(&self) -> &str {
        match self {
            Self::Ntsc => "NTSC",
            Self::Pal => "PAL",
            Self::Dendy => "Dendy",
        }
    }
}

impl TryFrom<&str> for NesRegion {
    type Error = NesError;

    fn try_from(value: &str) -> NesResult<Self> {
        match value {
            "NTSC" => Ok(Self::Ntsc),
            "PAL" => Ok(Self::Pal),
            "Dendy" => Ok(Self::Dendy),
            _ => Err(anyhow!("invalid nes region")),
        }
    }
}

impl From<usize> for NesRegion {
    fn from(value: usize) -> Self {
        match value {
            1 => Self::Pal,
            2 => Self::Dendy,
            _ => Self::Ntsc,
        }
    }
}

#[enum_dispatch(Mapper)]
pub trait Regional {
    fn region(&self) -> NesRegion {
        NesRegion::default()
    }
    fn set_region(&mut self, _region: NesRegion) {}
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum ResetKind {
    Soft,
    Hard,
}

#[enum_dispatch(Mapper)]
pub trait Reset {
    fn reset(&mut self, _kind: ResetKind) {}
}

#[enum_dispatch(Mapper)]
pub trait Clock {
    fn clock(&mut self) -> usize {
        0
    }
    fn clock_to(&mut self, _clocks: u64) {}
}

/// Trait for types that can output `f32` audio samples.
pub trait Sample {
    fn output(&self) -> f32;
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
        common::{NesRegion, Regional, Reset, ResetKind},
        control_deck::{Config, ControlDeck},
        input::Player,
        mapper::{Mapper, MapperRevision},
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
        path::{Path, PathBuf},
        sync::OnceLock,
    };
    use tetanes_util::NesResult;
    use tracing::debug;

    pub(crate) const RESULT_DIR: &str = "test_results";

    static PASS_DIR: OnceLock<PathBuf> = OnceLock::new();
    static FAIL_DIR: OnceLock<PathBuf> = OnceLock::new();

    #[macro_export]
    macro_rules! test_roms {
        ($directory:expr, $( $(#[ignore = $reason:expr])? $test:ident ),* $(,)?) => {$(
            $(#[ignore = $reason])?
            #[test]
            fn $test() -> NesResult<()> {
                $crate::common::tests::test_rom($directory, stringify!($test))
            }
        )*};
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[must_use]
    struct TestFrame {
        number: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hash: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        slot: Option<Player>,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<Action>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[must_use]
    struct RomTest {
        name: String,
        frames: Vec<TestFrame>,
    }

    fn get_rom_tests(directory: &str) -> NesResult<(PathBuf, Vec<RomTest>)> {
        let file = PathBuf::from(directory)
            .join("tests")
            .with_extension("json");
        let tests = File::open(&file)
            .and_then(|file| Ok(serde_json::from_reader::<_, Vec<RomTest>>(file)?))
            .with_context(|| format!("valid rom test data: {}", file.display()))?;
        Ok((file, tests))
    }

    fn load_control_deck<P: AsRef<Path>>(path: P) -> ControlDeck {
        let path = path.as_ref();
        let mut rom = File::open(path).expect("failed to open path");
        let mut deck = ControlDeck::with_config(Config {
            ram_state: RamState::AllZeros,
            filter: VideoFilter::Pixellate,
            ..Default::default()
        });
        deck.load_rom(&path.to_string_lossy(), &mut rom)
            .expect("failed to load rom");
        deck.set_filter(VideoFilter::Pixellate);
        deck.set_region(NesRegion::Ntsc);
        deck
    }

    fn on_frame_action(test_frame: &TestFrame, deck: &mut ControlDeck) {
        if let Some(action) = test_frame.action {
            debug!("{:?}", action);
            match action {
                Action::SoftReset => deck.reset(ResetKind::Soft),
                Action::HardReset => deck.reset(ResetKind::Hard),
                Action::MapperRevision(board) => match board {
                    MapperRevision::Mmc3(revision) => {
                        if let Mapper::Txrom(ref mut mapper) = deck.mapper_mut() {
                            mapper.set_revision(revision);
                        }
                    }
                    _ => panic!("unhandled MapperRevision {board:?}"),
                },
                Action::SetVideoFilter(filter) => deck.set_filter(filter),
                Action::SetNesRegion(format) => deck.set_region(format),
                Action::Joypad(button) => {
                    let slot = test_frame.slot.unwrap_or(Player::One);
                    let joypad = deck.joypad_mut(slot);
                    joypad.set_button(button, true);
                }
                _ => panic!("unhandled action: {action:?}"),
            }
        }
    }

    fn on_snapshot(
        test: &str,
        test_frame: &TestFrame,
        deck: &mut ControlDeck,
        count: usize,
    ) -> NesResult<Option<(u64, u64, u32, PathBuf)>> {
        match test_frame.hash {
            Some(expected) => {
                let mut hasher = DefaultHasher::new();
                let frame_buffer = deck.frame_buffer();
                frame_buffer.hash(&mut hasher);
                let actual = hasher.finish();
                debug!(
                    "frame : {}, matched: {}",
                    test_frame.number,
                    expected == actual
                );

                let result_dir = if env::var("UPDATE_SNAPSHOT").is_ok() || expected == actual {
                    PASS_DIR.get_or_init(|| {
                        let directory = PathBuf::from(RESULT_DIR).join("pass");
                        if let Err(err) = fs::create_dir_all(&directory) {
                            panic!(
                                "created pass test results dir: {}. {err}",
                                directory.display()
                            );
                        }
                        directory
                    })
                } else {
                    FAIL_DIR.get_or_init(|| {
                        let directory = PathBuf::from(RESULT_DIR).join("fail");
                        if let Err(err) = fs::create_dir_all(&directory) {
                            panic!(
                                "created fail test results dir: {}. {err}",
                                directory.display()
                            );
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

                ImageBuffer::<Rgba<u8>, &[u8]>::from_raw(Ppu::WIDTH, Ppu::HEIGHT, frame_buffer)
                    .expect("valid frame")
                    .save(&screenshot)
                    .with_context(|| {
                        format!("failed to save screenshot: {}", screenshot.display())
                    })?;

                Ok(Some((expected, actual, test_frame.number, screenshot)))
            }
            None => Ok(None),
        }
    }

    pub(crate) fn test_rom(directory: &str, test_name: &str) -> NesResult<()> {
        static INIT_TESTS: OnceLock<bool> = OnceLock::new();

        let initialized = INIT_TESTS.get_or_init(|| {
            let result_dir = PathBuf::from(RESULT_DIR);
            if result_dir.exists() {
                if let Err(err) = fs::remove_dir_all(&result_dir) {
                    panic!(
                        "failed to clear test results dir: {}. {err}",
                        result_dir.display()
                    );
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

        let rom = PathBuf::from(directory)
            .join(PathBuf::from(&test.name))
            .with_extension("nes");
        anyhow::ensure!(rom.exists(), "No test rom found for {rom:?}");

        let mut deck = load_control_deck(&rom);

        let mut results = Vec::new();
        for test_frame in test.frames.iter() {
            debug!("{} - {:?}", test_frame.number, deck.joypad_mut(Player::One));

            while deck.frame_number() < test_frame.number {
                deck.clock_frame().expect("valid frame clock");
                deck.clear_audio_samples();
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
                if let Some(ref mut frame) = test
                    .frames
                    .iter_mut()
                    .find(|frame| frame.number == frame_number)
                {
                    frame.hash = Some(actual);
                }
            }
            anyhow::ensure!(
                expected == actual,
                "mismatched snapshot for {rom:?} -> {screenshot:?}",
            );
        }
        if update_required {
            File::create(&test_file)
                .context("failed to open rom test file")
                .and_then(|file| {
                    serde_json::to_writer_pretty(file, &tests)
                        .context("failed to serialize rom data")
                })
                .with_context(|| format!("failed to update snapshot: {}", test_file.display()))?
        }

        Ok(())
    }

    mod cpu {
        use super::*;
        test_roms!(
            "../test_roms/cpu",
            branch_backward,
            nestest,
            ram_after_reset,
            regs_after_reset,
            branch_basics,
            branch_forward,
            dummy_reads,
            dummy_writes_oam,
            dummy_writes_ppumem,
            exec_space_apu,
            exec_space_ppuio,
            flag_concurrency,
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
            int_branch_delays_irq,
            int_cli_latency,
            int_irq_and_dma,
            int_nmi_and_brk,
            int_nmi_and_irq,
            overclock,
            sprdma_and_dmc_dma,
            sprdma_and_dmc_dma_512,
            timing_test,
        );
    }

    mod ppu {
        use super::*;
        test_roms!(
            "../test_roms/ppu",
            _240pee, // TODO: Run each test
            color,   // TODO: Test all color combinations
            ntsc_torture,
            oam_read,
            oam_stress,
            open_bus,
            palette,
            palette_ram,
            read_buffer,
            scanline,
            spr_hit_alignment,
            spr_hit_basics,
            spr_hit_corners,
            spr_hit_double_height,
            spr_hit_edge_timing,
            spr_hit_flip,
            spr_hit_left_clip,
            spr_hit_right_edge,
            spr_hit_screen_bottom,
            spr_hit_timing_basics,
            spr_hit_timing_order,
            spr_overflow_basics,
            spr_overflow_details,
            spr_overflow_emulator,
            spr_overflow_obscure,
            spr_overflow_timing,
            sprite_ram,
            tv,
            vbl_nmi_basics,
            vbl_nmi_clear_timing,
            vbl_nmi_control,
            vbl_nmi_disable,
            vbl_nmi_even_odd_frames,
            #[ignore = "clock is skipped too late relative to enabling BG Failed #3"]
            vbl_nmi_even_odd_timing,
            vbl_nmi_frame_basics,
            vbl_nmi_off_timing,
            vbl_nmi_on_timing,
            vbl_nmi_set_time,
            vbl_nmi_suppression,
            vbl_nmi_timing,
            vbl_timing,
            vram_access,
        );
    }

    mod apu {
        use super::*;

        test_roms!(
            "../test_roms/apu",
            clock_jitter,
            dmc_basics,
            dmc_dma_2007_read,
            dmc_dma_2007_write,
            dmc_dma_4016_read,
            dmc_dma_double_2007_read,
            dmc_dma_read_write_2007,
            dmc_rates,
            dpcmletterbox,
            irq_flag,
            #[ignore = "fails $04"]
            irq_flag_timing,
            irq_timing,
            jitter,
            len_ctr,
            #[ignore = "fails $03"]
            len_halt_timing,
            #[ignore = "fails $02"]
            len_reload_timing,
            len_table,
            #[ignore = "Channel: 0 second length of mode 0 is too soon"]
            len_timing,
            #[ignore = "fails $03"]
            len_timing_mode0,
            #[ignore = "fails $03"]
            len_timing_mode1,
            reset_4015_cleared,
            reset_4017_timing,
            reset_4017_written,
            reset_irq_flag_cleared,
            #[ignore = "At power, length counters should be enabled, #2"]
            reset_len_ctrs_enabled,
            reset_timing,
            reset_works_immediately,
            test_1,
            test_2,
            #[ignore = "failed"]
            test_3,
            #[ignore = "failed"]
            test_4,
            test_5,
            test_6,
            #[ignore = "failed"]
            test_7,
            #[ignore = "failed"]
            test_8,
            #[ignore = "failed"]
            test_9,
            #[ignore = "failed"]
            test_10,
            #[ignore = "fails #2"]
            pal_clock_jitter,
            pal_irq_flag,
            #[ignore = "fails #2"]
            pal_irq_flag_timing,
            #[ignore = "fails #3"]
            pal_irq_timing,
            pal_len_ctr,
            #[ignore = "fails #3"]
            pal_len_halt_timing,
            #[ignore = "fails #2"]
            pal_len_reload_timing,
            pal_len_table,
            #[ignore = "fails #3"]
            pal_len_timing_mode0,
            #[ignore = "fails #3"]
            pal_len_timing_mode1,
            #[ignore = "todo: compare output"]
            apu_env,
            #[ignore = "fails: fix silence"]
            dmc,
            #[ignore = "passes: check status"]
            dmc_buffer_retained,
            #[ignore = "todo: compare output"]
            dmc_latency,
            #[ignore = "todo: compare output"]
            dmc_pitch,
            #[ignore = "passes: check status"]
            dmc_status,
            #[ignore = "passes: todo, check status"]
            dmc_status_irq,
            #[ignore = "todo: compare output"]
            lin_ctr,
            #[ignore = "fails: fix silence"]
            noise,
            #[ignore = "todo: compare output"]
            noise_pitch,
            #[ignore = "todo: compare output"]
            phase_reset,
            #[ignore = "fails: fix silence"]
            square,
            #[ignore = "todo: compare output"]
            square_pitch,
            #[ignore = "todo: compare output"]
            sweep_cutoff,
            #[ignore = "todo: compare output"]
            sweep_sub,
            #[ignore = "fails: fix silence"]
            triangle,
            #[ignore = "todo: compare output"]
            triangle_pitch,
            #[ignore = "todo: compare output"]
            volumes,
        );
    }

    mod input {
        use super::*;
        test_roms!(
            "../test_roms/input",
            #[ignore = "todo"]
            zapper_flip,
            #[ignore = "todo"]
            zapper_light,
            #[ignore = "todo"]
            zapper_stream,
            #[ignore = "todo"]
            zapper_trigger,
        );
    }

    mod m004_txrom {
        use super::*;
        test_roms!(
            "../test_roms/mapper/m004_txrom",
            a12_clocking,
            clocking,
            details,
            rev_b,
            scanline_timing,
            big_chr_ram,
            rev_a,
        );
    }

    mod m005_exrom {
        use super::*;
        test_roms!("../test_roms/mapper/m005_exrom", exram, basics);
    }
}
