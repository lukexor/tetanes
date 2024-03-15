//! User Interface representing the the NES Control Deck

use config::Config;
use emulation::Emulation;
use event::{NesEvent, State};
use platform::{BuilderExt, EventLoopExt, WindowExt};
use renderer::{BufferPool, Renderer};
use std::sync::Arc;
use tetanes_util::{frame_begin, profile, NesResult};
use winit::{
    event_loop::{EventLoop, EventLoopBuilder},
    window::{Fullscreen, Window, WindowBuilder},
};

pub mod action;
pub mod audio;
pub mod config;
pub mod emulation;
pub mod event;
pub mod platform;
pub mod renderer;

/// Represents all the NES Emulation state.
#[derive(Debug)]
pub struct Nes {
    config: Config,
    window: Arc<Window>,
    emulation: Emulation,
    renderer: Renderer,
    #[cfg(target_arch = "wasm32")]
    event_proxy: winit::event_loop::EventLoopProxy<NesEvent>,
    state: State,
}

impl Nes {
    /// Begins emulation by starting the game engine loop.
    ///
    /// # Errors
    ///
    /// If engine fails to build or run, then an error is returned.
    pub async fn run(config: Config) -> NesResult<()> {
        // Set up window, events and NES state
        let event_loop = EventLoopBuilder::<NesEvent>::with_user_event().build()?;
        let mut nes = Nes::initialize(config, &event_loop).await?;
        event_loop
            .run_platform(move |event, window_target| nes.event_loop(event, window_target))?;

        Ok(())
    }

    /// Initializes the NES emulation.
    async fn initialize(config: Config, event_loop: &EventLoop<NesEvent>) -> NesResult<Self> {
        let window = Arc::new(Nes::initialize_window(event_loop, &config)?);
        let event_proxy = event_loop.create_proxy();
        let frame_pool = BufferPool::new();
        let state = State::new();
        let emulation =
            Emulation::initialize(event_proxy.clone(), frame_pool.clone(), config.clone())?;
        let renderer = Renderer::initialize(
            event_proxy.clone(),
            Arc::clone(&window),
            frame_pool,
            &config,
        )
        .await?;

        let mut nes = Self {
            config,
            window,
            emulation,
            renderer,
            #[cfg(target_arch = "wasm32")]
            event_proxy,
            state,
        };
        nes.initialize_platform()?;

        Ok(nes)
    }

    /// Initializes the window in a platform agnostic way.
    pub fn initialize_window(
        event_loop: &EventLoop<NesEvent>,
        config: &Config,
    ) -> NesResult<Window> {
        let (inner_size, min_inner_size) = config.inner_dimensions();
        let window_builder = WindowBuilder::new();
        let window_builder = window_builder
            .with_active(true)
            .with_inner_size(inner_size)
            .with_min_inner_size(min_inner_size)
            .with_title(Config::WINDOW_TITLE)
            // TODO: Support exclusive fullscreen config
            .with_fullscreen(config.fullscreen.then_some(Fullscreen::Borderless(None)))
            .with_resizable(false)
            .with_platform();
        let window = window_builder.build(event_loop)?;

        Ok(window)
    }

    fn next_frame(&mut self) {
        frame_begin!();
        profile!();
        if let Err(err) = self.emulation.request_clock_frame() {
            self.on_error(err);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::action::{Action, Setting, UiState};
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
    use tetanes_core::{
        common::{NesRegion, Regional, Reset, ResetKind},
        control_deck::{Config, ControlDeck},
        input::Player,
        mapper::{Mapper, MapperRevision},
        mem::RamState,
        ppu::Ppu,
        video::VideoFilter,
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
                $crate::nes::tests::test_rom($directory, stringify!($test))
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
        deck.load_rom(&path.to_string_lossy(), &mut rom, None)
            .expect("failed to load rom");
        deck.set_filter(VideoFilter::Pixellate);
        deck.set_region(NesRegion::Ntsc);
        deck
    }

    fn on_frame_action(test_frame: &TestFrame, deck: &mut ControlDeck) {
        if let Some(action) = test_frame.action {
            debug!("{:?}", action);
            match action {
                Action::Ui(state) => match state {
                    UiState::SoftReset => deck.reset(ResetKind::Soft),
                    UiState::HardReset => deck.reset(ResetKind::Hard),
                    UiState::MapperRevision(board) => match board {
                        MapperRevision::Mmc3(revision) => {
                            if let Mapper::Txrom(ref mut mapper) = deck.mapper_mut() {
                                mapper.set_revision(revision);
                            }
                        }
                        _ => panic!("unhandled MapperRevision {board:?}"),
                    },
                    _ => panic!("unhandled Nes state: {state:?}"),
                },
                Action::Setting(setting) => match setting {
                    Setting::SetVideoFilter(filter) => deck.set_filter(filter),
                    Setting::SetNesRegion(format) => deck.set_region(format),
                    _ => panic!("unhandled Setting: {setting:?}"),
                },
                Action::Joypad(button) => {
                    let slot = test_frame.slot.unwrap_or(Player::One);
                    let joypad = deck.joypad_mut(slot);
                    joypad.set_button(button, true);
                }
                _ => (),
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
        #[ignore = "fails $04"]
        len_reload_timing,
        len_table,
        #[ignore = "Channel: 0 second length of mode 0 is too soon"]
        len_timing,
        #[ignore = "fails $04"]
        len_timing_mode0,
        #[ignore = "fails $05"]
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
        #[ignore = "todo"]
        test_3,
        #[ignore = "todo"]
        test_4,
        test_5,
        test_6,
        #[ignore = "todo"]
        test_7,
        #[ignore = "todo"]
        test_8,
        #[ignore = "todo"]
        test_9,
        #[ignore = "todo"]
        test_10,
        #[ignore = "todo"]
        pal_clock_jitter,
        pal_irq_flag,
        #[ignore = "todo"]
        pal_irq_flag_timing,
        #[ignore = "todo"]
        pal_irq_timing,
        pal_len_ctr,
        #[ignore = "todo"]
        pal_len_halt_timing,
        #[ignore = "todo"]
        pal_len_reload_timing,
        pal_len_table,
        #[ignore = "todo"]
        pal_len_timing_mode0,
        #[ignore = "todo"]
        pal_len_timing_mode1,
    );
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
    test_roms!("../test_roms/mapper/m005_exrom", exram, basics);
}
