use crate::nes::{
    RunState,
    action::DebugStep,
    audio::{Audio, State as AudioState},
    config::{Config, FrameRate},
    emulation::{replay::Record, rewind::Rewind},
    event::{
        ConfigEvent, DebugEvent, DebugRequest, EmulationEvent, NesEvent, NesEventProxy,
        RendererEvent, UiEvent,
    },
    renderer::{
        FrameRecycle,
        gui::{
            MessageType,
            debugger::{AddressSpace, CpuSnapshot},
        },
    },
};
use anyhow::{Context, anyhow};
use chrono::Local;
use crossbeam::channel;
use egui::ViewportId;
use replay::Replay;
use std::{
    collections::VecDeque,
    io::{self, Read},
    path::{Path, PathBuf},
};
use tetanes_core::{
    apu::Apu,
    bus::Bus,
    common::{NesRegion, ResetKind},
    control_deck::{self, Clocked, ControlDeck, LoadedRom},
    cpu::Cpu,
    debug::{Access, Breakpoint, CodeMap},
    memory::{PRG_PAGES, Page},
    ppu,
    time::{Duration, Instant},
    video::Frame,
};
use thingbuf::mpsc::{blocking::Sender as BufSender, errors::TrySendError};
use tracing::{debug, error, trace};
use winit::event::ElementState;

pub mod replay;
pub mod rewind;

#[derive(Debug, Copy, Clone, PartialEq)]
#[must_use]
pub struct FrameStats {
    /// When this frame was measured, so samples can be plotted against a real time axis.
    pub timestamp: Instant,
    /// Frames per second, averaged over the sample window.
    pub fps: f32,
    /// The lowest frames per second in the sample window.
    pub fps_min: f32,
    /// Milliseconds per frame, averaged over the sample window.
    pub frame_time: f32,
    /// The longest frame in the sample window, in milliseconds.
    pub frame_time_max: f32,
    /// This frame alone, in milliseconds. The averages above hide the hitches a plot is for.
    pub frame_time_raw: f32,
    /// Frames emulated since stats were last reset.
    pub frame_count: usize,
    /// Frames emulated but never drawn, because the renderer had not claimed the previous one.
    pub dropped_frames: usize,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            timestamp: Instant::now(),
            fps: 0.0,
            fps_min: 0.0,
            frame_time: 0.0,
            frame_time_max: 0.0,
            frame_time_raw: 0.0,
            frame_count: 0,
            dropped_frames: 0,
        }
    }
}

impl FrameStats {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug)]
#[must_use]
pub struct FrameTimeDiag {
    frame_count: usize,
    dropped_frames: usize,
    history: VecDeque<f32>,
    sum: f32,
    avg: f32,
    last_update: Instant,
}

impl FrameTimeDiag {
    const MAX_HISTORY: usize = 120;
    const UPDATE_INTERVAL: Duration = Duration::from_millis(300);
    /// Frames to measure before reporting anything, so the average settles.
    ///
    /// The first interval after a reset is also a fraction of a frame rather than a whole one,
    /// and plotted it reads as a dip to zero.
    const WARMUP: usize = 10;

    fn new() -> Self {
        Self {
            frame_count: 0,
            dropped_frames: 0,
            history: VecDeque::with_capacity(Self::MAX_HISTORY),
            sum: 0.0,
            avg: 1.0 / 60.0,
            last_update: Instant::now(),
        }
    }

    const fn is_warm(&self) -> bool {
        self.frame_count >= Self::WARMUP
    }

    fn push(&mut self, frame_time: f32) {
        self.frame_count += 1;

        if frame_time.is_finite() && self.is_warm() {
            if self.history.len() >= Self::MAX_HISTORY
                && let Some(oldest) = self.history.pop_front()
            {
                self.sum -= oldest;
            }
            self.sum += frame_time;
            self.history.push_back(frame_time);
        }
    }

    fn avg(&mut self) -> f32 {
        if !self.history.is_empty() {
            let now = Instant::now();
            if now > self.last_update + Self::UPDATE_INTERVAL {
                self.last_update = now;
                self.avg = self.sum / self.history.len() as f32;
            }
        }
        self.avg
    }

    fn history(&self) -> impl Iterator<Item = &f32> {
        self.history.iter()
    }

    const fn drop_frame(&mut self) {
        self.dropped_frames += 1;
    }

    fn reset(&mut self) {
        self.frame_count = 0;
        self.dropped_frames = 0;
        self.history.clear();
        self.sum = 0.0;
        self.avg = 1.0 / 60.0;
        self.last_update = Instant::now();
    }
}

fn shutdown(tx: &NesEventProxy, err: impl std::fmt::Display) {
    error!("{err}");
    tx.event(UiEvent::Terminate);
}

#[derive(Debug)]
#[must_use]
enum Threads {
    Single(Box<Single>),
    Multi(Multi),
}

#[derive(Debug)]
#[must_use]
struct Single {
    state: State,
}

#[derive(Debug)]
#[must_use]
struct Multi {
    tx: channel::Sender<NesEvent>,
}

impl Multi {
    fn spawn(
        proxy_tx: NesEventProxy,
        frame_tx: BufSender<Frame, FrameRecycle>,
        cfg: &Config,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = channel::bounded(128);
        // The handle is dropped rather than kept: nothing joins this thread, and it ends itself
        // when the channel disconnects - which is when `Multi`, and so `tx`, is dropped.
        std::thread::Builder::new()
            .name("emulation".into())
            .spawn({
                let cfg = cfg.clone();
                move || Self::main(proxy_tx, rx, frame_tx, &cfg)
            })?;
        Ok(Self { tx })
    }

    fn main(
        tx: NesEventProxy,
        rx: channel::Receiver<NesEvent>,
        frame_tx: BufSender<Frame, FrameRecycle>,
        cfg: &Config,
    ) {
        debug!("emulation thread started");
        let mut state = State::new(tx, frame_tx, cfg); // Has to be created on the thread, since
        loop {
            while let Ok(event) = rx.try_recv() {
                state.on_event(&event);
            }

            // Wait on the channel rather than parking the thread.
            //
            // `unpark` leaves a *sticky* token: unparking a thread that is not currently parked
            // makes its next `park_timeout` return immediately, however long it was asked for. The
            // UI thread unparked on every event and on every redraw, so a compositor delivering a
            // burst - focus-follows-mouse over the window will do it - left this loop unable to
            // sleep at all, spinning through wake-ups while the frame it owed went unclocked.
            // Blocking on the channel wakes exactly once per event and honours the timeout
            // otherwise.
            if let Some(timeout) = state.try_clock_frame() {
                match rx.recv_timeout(timeout) {
                    Ok(event) => state.on_event(&event),
                    Err(channel::RecvTimeoutError::Timeout) => (),
                    Err(channel::RecvTimeoutError::Disconnected) => {
                        debug!("emulation channel disconnected");
                        break;
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct Emulation {
    threads: Threads,
}

impl Emulation {
    /// Initializes the renderer in a platform-agnostic way.
    pub fn new(
        tx: NesEventProxy,
        frame_tx: BufSender<Frame, FrameRecycle>,
        cfg: &Config,
    ) -> anyhow::Result<Self> {
        let threaded = cfg.emulation.threaded
            && std::thread::available_parallelism().is_ok_and(|count| count.get() > 1);
        let backend = if threaded {
            Threads::Multi(Multi::spawn(tx, frame_tx, cfg)?)
        } else {
            Threads::Single(Box::new(Single {
                state: State::new(tx, frame_tx, cfg),
            }))
        };

        Ok(Self { threads: backend })
    }

    /// Handle event.
    pub fn on_event(&mut self, event: &NesEvent) {
        match &mut self.threads {
            Threads::Single(single) => single.state.on_event(event),
            // Sending is the wake-up: the thread is blocked on this channel, not parked.
            Threads::Multi(Multi { tx, .. }) => {
                if let Err(err) = tx.try_send(event.clone()) {
                    error!("failed to send emulation event: {event:?}. {err:?}");
                }
            }
        }
    }

    pub fn try_clock_frame(&mut self) {
        match &mut self.threads {
            Threads::Single(single) => {
                // The event loop is the clock here, so whatever it reports is not ours to wait on.
                let _ = single.state.try_clock_frame();
            }
            // Multi-threaded emulation paces itself on the wall clock and asks for its own
            // redraws, so a redraw on the UI thread is not a reason to disturb it.
            Threads::Multi(_) => (),
        }
    }

    pub fn terminate(&mut self) {
        match &mut self.threads {
            Threads::Single(_) => (),
            Threads::Multi(Multi { tx, .. }) => {
                if let Err(err) = tx.try_send(NesEvent::Ui(UiEvent::Terminate)) {
                    error!("failed to send termination event. {err:?}");
                }
            }
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct State {
    tx: NesEventProxy,
    control_deck: ControlDeck,
    audio: Audio,
    frame_tx: BufSender<Frame, FrameRecycle>,
    frame_latency: usize,
    target_frame_duration: Duration,
    /// When the next frame is due.
    ///
    /// An absolute deadline rather than an accumulated remainder: oversleeping one frame is then
    /// corrected on the next rather than accumulating, and nothing has to spin out the last
    /// fraction of a millisecond to stay in step.
    next_frame_time: Instant,
    last_frame_time: Instant,
    /// When the renderer was last asked to draw, so the retry for an unclaimed frame can be
    /// throttled to once a frame.
    last_redraw_request: Instant,
    /// Running estimate of how far the nominal output rate is from what the sound card consumes.
    ///
    /// See [`State::update_audio_rate`].
    audio_rate_bias: f32,
    frame_time_diag: FrameTimeDiag,
    run_state: RunState,
    /// What the Debugger wants each frame, if it is open.
    debug_request: Option<DebugRequest>,
    /// The PRG mapping the address space was last captured as, so the capture is skipped when the
    /// board has not bank switched since.
    debug_pages: Option<[Page; PRG_PAGES]>,
    /// The code map generation the address space was last captured at, so the capture is also
    /// redone when execution has revealed an instruction the last one collapsed as unknown.
    debug_generation: Option<u64>,
    /// What the code map recorded before the debugger closed, kept so that reopening it does not
    /// start over. Only recording stops. The marks stay true for as long as the cart is loaded.
    debug_code_map: Option<CodeMap>,
    /// Addresses to stop the console at, empty unless the Debugger has armed some.
    ///
    /// Checking them means clocking the console an instruction at a time, so an empty list is what
    /// keeps a console with no breakpoints on the frame-at-a-time path.
    debug_breakpoints: Vec<Breakpoint>,
    threaded: bool,
    rewinding: bool,
    rewind: Rewind,
    record: Record,
    replay: Replay,
    save_slot: u8,
    auto_save: bool,
    auto_save_interval: Duration,
    last_auto_save: Instant,
    auto_load: bool,
    /// The last cart that loaded successfully, which is what tells a swap from a first load.
    ///
    /// Not `ControlDeck::loaded_rom`, which a failed load leaves empty: opening a ROM that will not
    /// load between two games would otherwise look like a first load, and the previous game's
    /// cheats would carry into the next one.
    last_rom: Option<String>,
    speed: f32,
    run_ahead: usize,
    show_frame_stats: bool,
    /// Frame-time accounting for `--bench`, absent in normal play.
    bench: Option<Bench>,
}

impl Drop for State {
    fn drop(&mut self) {
        self.unload_rom();
    }
}

/// Frame-time accounting for `--bench`.
///
/// Records the wall interval between finished frames, which is throughput rather than the cost of
/// any one frame: threaded, that is the emulation thread's own rate, and single-threaded it is
/// emulation plus render plus present, because the event loop does all three in turn.
#[derive(Debug)]
#[must_use]
struct Bench {
    /// Untimed frames still owed before the clock starts, so the measurement is not of boot.
    warmup: u32,
    /// Timed frames still owed.
    remaining: u32,
    /// Interval of each timed frame, in milliseconds.
    intervals: Vec<f32>,
    /// When the previous frame finished, or `None` before the first timed frame.
    last: Option<Instant>,
}

impl Bench {
    /// Frames clocked before timing starts. Matches `clock_frame`'s default so the two benchmarks
    /// are measuring the same part of the run.
    const WARMUP_FRAMES: u32 = 120;

    fn new(frames: u32) -> Self {
        Self {
            warmup: Self::WARMUP_FRAMES,
            remaining: frames,
            intervals: Vec::with_capacity(frames as usize),
            last: None,
        }
    }

    /// Records a finished frame, returning true once the last timed frame is in.
    fn frame_done(&mut self) -> bool {
        if self.warmup > 0 {
            self.warmup -= 1;
            return false;
        }
        let now = Instant::now();
        // The first timed frame only starts the clock: an interval needs two endpoints, and the
        // one before it is the last warmup frame, which was not measured.
        if let Some(last) = self.last.replace(now) {
            self.intervals.push((now - last).as_secs_f32() * 1000.0);
            self.remaining -= 1;
        }
        self.remaining == 0
    }

    /// Prints the same statistics `clock_frame` reports, so the two can be read side by side.
    fn report(&self, threaded: bool) {
        let n = self.intervals.len() as f32;
        if n == 0.0 {
            println!("no frames were timed");
            return;
        }
        let mean = self.intervals.iter().sum::<f32>() / n;
        let var = self
            .intervals
            .iter()
            .map(|t| (t - mean).powi(2))
            .sum::<f32>()
            / n;
        let stddev = var.sqrt();
        let (min, max) = self
            .intervals
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), t| {
                (lo.min(*t), hi.max(*t))
            });
        let scope = if threaded {
            "emulation thread"
        } else {
            "emulation + render"
        };
        println!("\n=== RESULTS ({scope}) ===");
        println!(
            "{:>10} {:>10} {:>8} {:>10} {:>10}",
            "ms/frame", "stddev", "cv", "min", "max"
        );
        println!(
            "{mean:>10.3} {stddev:>10.4} {:>7.2}% {min:>10.3} {max:>10.3}",
            100.0 * stddev / mean
        );
    }
}

impl State {
    /// How many frames of a stall may be repaid before the debt is written off.
    const MAX_CATCHUP_FRAMES: u32 = 3;
    /// Largest fraction the audio output rate may be bent by, `d` in Arntzen (2012).
    ///
    /// The paper finds 0.002..=0.005 satisfactory and this is the top of that range: pacing on
    /// the wall clock is a less precise estimate of the true frame rate than a vsync-driven one,
    /// so it needs the headroom.
    const AUDIO_MAX_DEVIATION: f32 = 0.005;
    /// How fast the base-rate estimate tracks a persistent error, and how far it may go.
    ///
    /// Slow on purpose. The proportional term settles over a few hundred frames, so this has to
    /// be slower still or the two fight; more importantly, it must not mistake a *transient* for
    /// a rate error. At this gain a 60-frame dropout moves the estimate by well under a tenth of
    /// its range and it decays back, while a genuine mismatch is tracked out in ~20 seconds.
    const AUDIO_BIAS_GAIN: f32 = 3e-5;
    const AUDIO_MAX_BIAS: f32 = 0.005;

    fn new(tx: NesEventProxy, frame_tx: BufSender<Frame, FrameRecycle>, cfg: &Config) -> Self {
        let mut control_deck = ControlDeck::with_config(cfg.deck.clone());
        let audio = Audio::new(
            cfg.audio.enabled,
            Apu::DEFAULT_SAMPLE_RATE,
            cfg.audio.latency,
            cfg.audio.buffer_size,
        );
        if cfg.audio.enabled && audio.device().is_none() {
            tx.event(ConfigEvent::AudioEnabled(false));
            tx.event(UiEvent::Message((
                MessageType::Warn,
                "No audio device found.".into(),
            )));
        }
        if Apu::DEFAULT_SAMPLE_RATE != audio.sample_rate {
            control_deck.set_sample_rate(audio.sample_rate);
        }
        let rewind = Rewind::new(
            cfg.emulation.rewind,
            cfg.emulation.rewind_seconds,
            cfg.emulation.rewind_interval,
        );
        let target_frame_duration = FrameRate::from(cfg.deck.region).duration();
        let mut state = Self {
            tx,
            control_deck,
            audio,
            frame_tx,
            frame_latency: 1,
            target_frame_duration,
            next_frame_time: Instant::now(),
            last_frame_time: Instant::now(),
            last_redraw_request: Instant::now(),
            audio_rate_bias: 1.0,
            frame_time_diag: FrameTimeDiag::new(),
            run_state: RunState::AutoPaused,
            debug_request: None,
            debug_pages: None,
            debug_generation: None,
            debug_code_map: None,
            debug_breakpoints: Vec::new(),
            threaded: cfg.emulation.threaded
                && std::thread::available_parallelism().is_ok_and(|count| count.get() > 1),
            rewinding: false,
            rewind,
            record: Record::new(),
            replay: Replay::new(),
            save_slot: cfg.emulation.save_slot,
            auto_save: cfg.emulation.auto_save,
            auto_save_interval: cfg.emulation.auto_save_interval,
            last_auto_save: Instant::now(),
            auto_load: cfg.emulation.auto_load,
            last_rom: None,
            speed: cfg.emulation.speed,
            run_ahead: cfg.emulation.run_ahead,
            show_frame_stats: false,
            bench: cfg.emulation.bench.map(Bench::new),
        };
        state.update_region(cfg.deck.region);
        state.update_run_ahead();
        state
    }

    pub(crate) fn add_message<S: ToString>(&mut self, ty: MessageType, msg: S) {
        self.tx.event(UiEvent::Message((ty, msg.to_string())));
    }

    /// Pushes the configured run-ahead down to the deck, disabling it while rewinding, which
    /// replays recorded states and so has no input latency to hide.
    ///
    /// The speed rule is not here: `ControlDeck` applies run-ahead only at 1x on its own, since
    /// speculating past the current frame only means anything when a display frame is one frame.
    const fn update_run_ahead(&mut self) {
        self.control_deck
            .set_run_ahead(if self.rewinding { 0 } else { self.run_ahead });
    }

    /// Start or stop rewinding, keeping run-ahead in step with it.
    const fn set_rewinding(&mut self, rewinding: bool) {
        self.rewinding = rewinding;
        self.update_run_ahead();
    }

    fn write_deck<T>(
        &mut self,
        writer: impl FnOnce(&mut ControlDeck) -> control_deck::Result<T>,
    ) -> Option<T> {
        writer(&mut self.control_deck)
            .map_err(|err| self.on_error(err))
            .ok()
    }

    fn on_error(&mut self, err: impl Into<anyhow::Error>) {
        let err = err.into();
        error!("Emulation error: {err:?}");
        if self.control_deck.cpu_corrupted() {
            let bus = self.control_deck.bus();
            let opcode = bus.peek(bus.cpu.pc.wrapping_sub(1));
            self.tx.event(EmulationEvent::CpuCorrupted {
                instr: Cpu::INSTR_REF[usize::from(opcode)],
            });
        } else {
            self.add_message(MessageType::Error, err);
        }
    }

    /// Handle event.
    fn on_event(&mut self, event: &NesEvent) {
        match event {
            NesEvent::Ui(UiEvent::Terminate) => {
                self.unload_rom();
                debug!("emulation stopped");
            }
            NesEvent::Emulation(event) => self.on_emulation_event(event),
            NesEvent::Config(event) => self.on_config_event(event),
            _ => (),
        }
    }

    /// Handle emulation event.
    fn on_emulation_event(&mut self, event: &EmulationEvent) {
        match event {
            EmulationEvent::DebugSubscribe(request) => {
                let resubscribing = self.debug_request.is_some() && request.is_some();
                self.debug_request = *request;
                // Executed instructions can only be collected as they run, so start recording when
                // the subscription starts, if history is requested. A re-subscribe, which every
                // pane toggle sends, keeps what has been recorded so far.
                let history = request
                    .filter(|request| request.history_lines > 0)
                    .map(|request| usize::from(request.history_lines));
                if !resubscribing || history.is_none() {
                    self.control_deck.set_pc_history(history);
                }
                // Which bytes are instructions is marked by running them, so the map starts here
                // too. A first subscription starts empty until the game runs, leaving the
                // disassembly one large unknown block plus whatever PC points at. Closing the
                // debugger stops the recording but keeps the marks, so reopening it does not start
                // over.
                if request.is_some() {
                    // Attaching again would hand `None` back and start a fresh map, throwing away
                    // every mark the session has made.
                    if !resubscribing {
                        let recorded = self.debug_code_map.take();
                        self.control_deck.attach_code_map(recorded);
                    }
                } else {
                    self.debug_code_map = self.control_deck.detach_code_map();
                    // Nothing would report a stop with the Debugger closed, so the console would
                    // sit paused with no way to see why. The list itself is the Debugger's, and
                    // comes back when it reopens.
                    self.debug_breakpoints.clear();
                    self.control_deck.set_breakpoints([]);
                }
                // A fresh subscription has nothing to compare against, so ensure an updated address
                // space is sent.
                self.debug_pages = None;
                self.debug_generation = None;
                self.send_address_space();
                self.send_debug_snapshot();
            }
            EmulationEvent::DebugBreakpoints(breakpoints) => {
                self.debug_breakpoints.clone_from(breakpoints);
                // Reads and writes are caught on the bus, execution between instructions, so the
                // console is told only the half it can see.
                self.control_deck.set_breakpoints(
                    breakpoints
                        .iter()
                        .filter(|breakpoint| {
                            // Execution stops between instructions, so the deck sees it only for
                            // the breakpoints that record instead of stopping.
                            let bus_side = if breakpoint.breaks {
                                Access::READ | Access::WRITE
                            } else {
                                Access::all()
                            };
                            breakpoint.access.intersects(bus_side)
                        })
                        .map(|breakpoint| Breakpoint {
                            access: if breakpoint.breaks {
                                breakpoint.access & (Access::READ | Access::WRITE)
                            } else {
                                breakpoint.access
                            },
                            ..*breakpoint
                        }),
                );
            }
            EmulationEvent::AddDebugger(debugger) => {
                self.control_deck.set_debugger(debugger.clone());
            }
            EmulationEvent::RemoveDebugger => {
                self.control_deck.clear_debugger();
            }
            EmulationEvent::AudioRecord(recording) => {
                if self.control_deck.is_running() {
                    self.audio_record(*recording);
                }
            }
            EmulationEvent::CpuCorrupted { .. } => (), // Ignore, as only this module emits this
            // event
            EmulationEvent::DebugStep(step) => {
                if self.control_deck.is_running() {
                    match step {
                        DebugStep::Into => {
                            self.write_deck(|deck| deck.clock_instr());
                            self.send_frame();
                        }
                        DebugStep::Out => {
                            // Waiting on the stack pointer alone stops in the middle of returning
                            // from a subroutine, where it pulls saved registers back off the
                            // stack. The stack frame has not been left until the return address
                            // itself is pulled, which only a return instruction does.
                            let sp = self.control_deck.bus().cpu.sp;
                            self.step_until(|deck, opcode| {
                                matches!(opcode, Cpu::RTS | Cpu::RTI) && deck.bus().cpu.sp > sp
                            });
                            self.send_frame();
                        }
                        DebugStep::Over => {
                            // Only a jump has anything to step over.
                            let &Cpu { pc, sp, .. } = &self.control_deck.bus().cpu;
                            let is_jsr = self.control_deck.bus().peek(pc) == Cpu::JSR;
                            self.write_deck(|deck| deck.clock_instr());
                            if is_jsr {
                                // The stack pointer is below where the call left it, and the only
                                // thing that brings it back up to `sp` is pulling that return
                                // address.
                                self.step_until(|deck, _| deck.bus().cpu.sp >= sp);
                            }
                            self.send_frame();
                        }
                        DebugStep::Scanline => {
                            if self.write_deck(|deck| deck.clock_scanline()).is_some() {
                                self.send_frame();
                            }
                        }
                        DebugStep::Frame => {
                            // One NES frame, which stepping means regardless of the speed a
                            // display frame would clock.
                            if self
                                .write_deck(|deck| deck.clock_frame().map(|_| ()))
                                .is_some()
                            {
                                self.send_frame();
                            }
                        }
                    }
                    // A step reports where it landed, so an access it made is already on screen.
                    // Leaving the hit would stop the next resume on the first instruction and
                    // name an address from before the step.
                    self.control_deck.take_access_hit();
                    // A step can cross a bank switch, which moves every instruction after it.
                    self.send_address_space();
                }
            }
            EmulationEvent::InstantRewind => {
                if self.control_deck.is_running() {
                    self.instant_rewind();
                }
            }
            EmulationEvent::Joypad((player, button, state)) => {
                if self.control_deck.is_running() {
                    let pressed = *state == ElementState::Pressed;
                    let joypad = self.control_deck.joypad_mut(*player);
                    joypad.set_button(*button, pressed);
                    self.record
                        .push(self.control_deck.frame_number(), event.clone());
                }
            }
            EmulationEvent::LoadReplay((name, replay)) => {
                if self.control_deck.is_running() {
                    self.load_replay(name, &mut io::Cursor::new(replay));
                }
            }
            EmulationEvent::LoadReplayPath(path) => {
                if self.control_deck.is_running() {
                    self.load_replay_path(path);
                }
            }
            EmulationEvent::LoadRom((name, rom)) => {
                self.load_rom(name, &mut io::Cursor::new(rom));
            }
            EmulationEvent::LoadRomPath(path) => self.load_rom_path(path),
            EmulationEvent::LoadState(slot) => self.load_state(*slot),
            EmulationEvent::RunState(mode) => self.set_run_state(*mode),
            EmulationEvent::ReplayRecord(recording) => {
                if self.control_deck.is_running() {
                    self.replay_record(*recording);
                }
            }
            EmulationEvent::Reset(kind) => {
                self.reset_frame_stats();
                if self.control_deck.is_running() || self.control_deck.cpu_corrupted() {
                    self.control_deck.reset(*kind);
                    match kind {
                        ResetKind::Soft => self.add_message(MessageType::Info, "Reset"),
                        ResetKind::Hard => self.add_message(MessageType::Info, "Power Cycled"),
                    }
                }
            }
            EmulationEvent::RequestFrame => self.send_frame(),
            EmulationEvent::Rewinding(rewind) => {
                if self.control_deck.is_running() {
                    if self.rewind.enabled {
                        self.set_rewinding(*rewind);
                        if self.rewinding {
                            self.add_message(MessageType::Info, "Rewinding...");
                        }
                    } else {
                        self.rewind_disabled();
                    }
                }
            }
            EmulationEvent::SaveState(slot) => self.save_state(*slot, false),
            EmulationEvent::ShowFrameStats(show) => {
                self.reset_frame_stats();
                self.show_frame_stats = *show;
            }
            EmulationEvent::ResetFrameStats => self.reset_frame_stats(),
            EmulationEvent::Screenshot => {
                if self.control_deck.is_running() {
                    match self.save_screenshot() {
                        Ok(filename) => {
                            self.add_message(
                                MessageType::Info,
                                format!("Screenshot Saved: {}", filename.display()),
                            );
                        }
                        Err(err) => self.on_error(err),
                    }
                }
            }
            EmulationEvent::UnloadRom => self.unload_rom(),
            EmulationEvent::ZapperAim((x, y)) => {
                self.control_deck.aim_zapper(*x, *y);
                self.record
                    .push(self.control_deck.frame_number(), event.clone());
            }
            EmulationEvent::ZapperTrigger => {
                self.control_deck.trigger_zapper();
                self.record
                    .push(self.control_deck.frame_number(), event.clone());
            }
        }
    }

    /// Handle config event.
    fn on_config_event(&mut self, event: &ConfigEvent) {
        match event {
            ConfigEvent::ApuChannelEnabled((channel, enabled)) => {
                let prev_enabled = self.control_deck.apu_channel_enabled(*channel);
                self.control_deck
                    .set_apu_channel_enabled(*channel, *enabled);
                if prev_enabled != *enabled {
                    let enabled_text = if *enabled { "Enabled" } else { "Disabled" };
                    self.add_message(
                        MessageType::Info,
                        format!("{enabled_text} APU Channel {channel:?}"),
                    );
                }
            }
            ConfigEvent::AudioBuffer(buffer_size) => {
                if let Err(err) = self.audio.set_buffer_size(*buffer_size) {
                    self.on_error(err);
                }
            }
            ConfigEvent::AudioEnabled(enabled) => match self.audio.set_enabled(*enabled) {
                Ok(state) => match state {
                    AudioState::Started => self.add_message(MessageType::Info, "Audio Enabled"),
                    AudioState::Disabled | AudioState::Stopped => {
                        self.add_message(MessageType::Info, "Audio Disabled")
                    }
                    AudioState::NoOutputDevice => (),
                },
                Err(err) => self.on_error(err),
            },
            ConfigEvent::AudioLatency(latency) => {
                if let Err(err) = self.audio.set_latency(*latency) {
                    self.on_error(err);
                }
            }
            ConfigEvent::AutoLoad(enabled) => self.auto_load = *enabled,
            ConfigEvent::AutoSave(enabled) => self.auto_save = *enabled,
            ConfigEvent::AutoSaveInterval(interval) => self.auto_save_interval = *interval,
            ConfigEvent::ConcurrentDpad(enabled) => {
                self.control_deck.set_concurrent_dpad(*enabled);
            }
            ConfigEvent::EmulatePpuWarmup(enabled) => {
                self.control_deck.set_emulate_ppu_warmup(*enabled);
            }
            ConfigEvent::FourPlayer(four_player) => {
                self.control_deck.set_four_player(*four_player);
            }
            ConfigEvent::GenieCodeAdded(genie_code) => {
                self.control_deck.add_patch(genie_code.into());
            }
            ConfigEvent::GenieCodeRemoved(code) => {
                self.control_deck.remove_genie_code(code);
            }
            ConfigEvent::GenieCodeClear => {
                self.control_deck.clear_genie_codes();
            }
            ConfigEvent::RamState(ram_state) => {
                self.control_deck.set_ram_state(*ram_state);
            }
            ConfigEvent::Region(region) => {
                self.control_deck.set_region(*region);
                self.update_region(*region);
            }
            ConfigEvent::RewindEnabled(enabled) => self.rewind.set_enabled(*enabled),
            ConfigEvent::RewindInterval(interval) => self.rewind.set_interval(*interval),
            ConfigEvent::RewindSeconds(seconds) => self.rewind.set_seconds(*seconds),
            ConfigEvent::RunAhead(run_ahead) => {
                self.run_ahead = *run_ahead;
                self.update_run_ahead();
            }
            ConfigEvent::MapperRevisions(revs) => {
                self.control_deck.set_mapper_revisions(*revs);
            }
            ConfigEvent::SaveSlot(slot) => self.save_slot = *slot,
            ConfigEvent::Speed(speed) => {
                self.speed = *speed;
                self.control_deck.set_frame_speed(*speed);
                self.update_run_ahead();
            }
            ConfigEvent::VideoFilter(filter) => self.control_deck.set_filter(*filter),
            ConfigEvent::ZapperConnected(connected) => {
                self.control_deck.connect_zapper(*connected);
            }
            _ => (),
        }
    }

    /// Counts a frame against `--bench`, reporting and shutting down once the last one is in.
    fn update_bench(&mut self) {
        let threaded = self.threaded;
        if let Some(bench) = &mut self.bench
            && bench.frame_done()
        {
            bench.report(threaded);
            self.tx.event(UiEvent::Terminate);
        }
    }

    /// Starts the frame time history over, without the gap since it was last measured.
    ///
    /// Whatever stalled or idled in between is not a frame time, and left in it would be both the
    /// window's maximum and the whole scale of its plot.
    fn reset_frame_stats(&mut self) {
        self.frame_time_diag.reset();
        self.last_frame_time = Instant::now();
    }

    fn update_frame_stats(&mut self) {
        if !self.show_frame_stats {
            return;
        }

        let frame_time_raw = self.last_frame_time.elapsed().as_secs_f32();
        self.frame_time_diag.push(frame_time_raw);
        self.last_frame_time = Instant::now();
        if !self.frame_time_diag.is_warm() {
            return;
        }

        let frame_time = self.frame_time_diag.avg();
        let frame_time_max = self
            .frame_time_diag
            .history()
            .fold(-f32::INFINITY, |a, b| a.max(*b));
        let mut fps = 1.0 / frame_time;
        let mut fps_min = 1.0 / frame_time_max;
        if !fps.is_finite() {
            fps = 0.0;
        }
        if !fps_min.is_finite() {
            fps_min = 0.0;
        }
        self.tx.event(RendererEvent::FrameStats(FrameStats {
            timestamp: Instant::now(),
            fps,
            fps_min,
            frame_time: frame_time * 1000.0,
            frame_time_max: frame_time_max * 1000.0,
            frame_time_raw: frame_time_raw * 1000.0,
            frame_count: self.frame_time_diag.frame_count,
            dropped_frames: self.frame_time_diag.dropped_frames,
        }));
    }

    fn send_frame(&mut self) {
        match self.frame_tx.try_send_ref() {
            Ok(mut frame) => self.control_deck.frame_buffer_into(frame.as_array_mut()),
            Err(TrySendError::Full(_)) => {
                trace!("dropped frame");
                self.frame_time_diag.drop_frame();
            }
            Err(_) => shutdown(&self.tx, "failed to get frame"),
        }
        self.send_debug_snapshot();
    }

    /// Clock instructions until `done`, or until a default budget is spent.
    ///
    /// Bounded by budget because a subroutine that never returns - a crash, or a wait loop - would
    /// otherwise never finish, and on the single-threaded backend would block the UI thread. The
    /// budget is a few seconds of emulated time, far longer than any subroutine worth stepping
    /// over.
    fn step_until(&mut self, done: impl Fn(&ControlDeck, u8) -> bool) {
        const BUDGET: usize = 5_000_000;

        for _ in 0..BUDGET {
            let pc = self.control_deck.bus().cpu.pc;
            let opcode = self.control_deck.bus().peek(pc);
            if self.write_deck(|deck| deck.clock_instr()).is_none() {
                return;
            }
            if done(&self.control_deck, opcode) {
                return;
            }
        }
        self.add_message(
            MessageType::Warn,
            "Step timed out - the subroutine has not returned.",
        );
    }

    /// Send the Debugger a snapshot, if it's open.
    fn send_debug_snapshot(&mut self) {
        if let Some(request) = &self.debug_request {
            let mut snapshot = CpuSnapshot::capture(self.control_deck.bus(), request);
            snapshot.access_log = self.control_deck.drain_access_log();
            self.tx.event(DebugEvent::Cpu(Box::new(snapshot)));
        }
    }

    /// Send the address space if anything it is built from has changed since the last send.
    ///
    /// Only called when the console is stopped - opening the debugger, stepping, pausing.
    fn send_address_space(&mut self) {
        if self.debug_request.is_none() {
            return;
        }
        let pages = *self.control_deck.bus().memory.prg_pages();
        let generation = self.control_deck.code_map().map(CodeMap::generation);
        if self.debug_pages == Some(pages) && self.debug_generation == generation {
            return;
        }
        self.debug_pages = Some(pages);
        self.debug_generation = generation;
        let address_space = AddressSpace::capture(self.control_deck.bus());
        self.tx
            .event(DebugEvent::AddressSpace(Box::new(address_space)));
    }

    /// Stop the console at a breakpoint and tell the UI which one it was.
    fn on_breakpoint(&mut self, addr: u16) {
        // Stopped here rather than by asking the UI to pause us: that request would have to go
        // round the event loop, and on the threaded backend the console would run on - past the
        // breakpoint by however many frames the round trip took - before it arrived.
        self.set_run_state(RunState::ManuallyPaused);
        // Half a frame's worth of pixels, which the console has drawn at this point.
        self.send_frame();
        // An access is caught part way through an instruction and reported at the boundary after
        // it, so it names what was touched rather than where PC now sits.
        match self.control_deck.take_access_hit() {
            Some(hit) => self.tx.event(DebugEvent::AccessBreak(hit)),
            None => self.tx.event(DebugEvent::Breakpoint(addr)),
        }
    }

    fn set_run_state(&mut self, mode: RunState) {
        if !self.control_deck.cpu_corrupted() {
            self.run_state = mode;
            if self.run_state.paused() {
                if let Some(rom) = self.control_deck.loaded_rom()
                    && let Err(err) = self.record.stop(&rom.name)
                {
                    self.on_error(err);
                }
                self.send_address_space();
                self.send_debug_snapshot();
            } else {
                self.last_auto_save = Instant::now();
                // To avoid having a large dip in frame stats when unpausing
                self.last_frame_time = Instant::now();
            }
            self.audio.pause(self.run_state.paused());
        }
    }

    fn save_state(&mut self, slot: u8, auto: bool) {
        if let Some(rom) = self.control_deck.loaded_rom() {
            let data_dir = Config::save_path(&rom.name, slot);
            match self.control_deck.save_state_path(data_dir) {
                Ok(_) => {
                    if !auto {
                        self.add_message(MessageType::Info, format!("State {slot} Saved"));
                    }
                }
                Err(err) => self.on_error(err),
            }
        }
    }

    fn load_state(&mut self, slot: u8) {
        if let Some(rom) = self.control_deck.loaded_rom() {
            let save_path = Config::save_path(&rom.name, slot);
            match self.control_deck.load_state_path(save_path) {
                Ok(_) => self.add_message(MessageType::Info, format!("State {slot} Loaded")),
                Err(control_deck::Error::NoSaveStateFound) => {
                    self.add_message(MessageType::Warn, format!("State {slot} Not Found"));
                }
                Err(err) => {
                    self.on_error(err);
                }
            }
        }
    }

    fn unload_rom(&mut self) {
        if let Some(rom) = self.control_deck.loaded_rom() {
            if self.auto_save {
                let save_path = Config::save_path(&rom.name, self.save_slot);
                if let Err(err) = self.control_deck.save_state_path(save_path) {
                    self.on_error(err);
                }
            }
            self.replay_record(false);
            self.rewind.clear();
            // Marks are offsets into this cart's memory. Both load paths come through here, so a
            // kept map cannot outlive the cart it describes.
            self.debug_code_map = None;
            let _ = self.audio.stop();
            if let Err(err) = self.control_deck.unload_rom() {
                self.on_error(err);
            }
            self.tx.event(RendererEvent::RomUnloaded);
            self.tx.event(RendererEvent::RequestRedraw {
                viewport_id: ViewportId::ROOT,
                when: Instant::now(),
            });
            self.reset_frame_stats();
        }
    }

    fn on_load_rom(&mut self, rom: LoadedRom) {
        // A cheat belongs to the cart it was entered for: the same address in another game holds
        // something else entirely, so the codes come out with the cartridge. Only on a *change* -
        // a first load has to keep what `--genie-code` and the config put there for the game about
        // to start, and quitting must not wipe the saved list, which both a plain unload hook and
        // a plain load hook would do.
        let swapped = self.last_rom.as_ref().is_some_and(|name| *name != rom.name);
        self.last_rom = Some(rom.name.clone());
        if swapped && self.control_deck.patches().count() > 0 {
            // Cleared here as well as through the event, so the new game does not run its opening
            // frames under the last one's codes while the event travels to the config and back.
            self.control_deck.clear_genie_codes();
            self.tx.event(ConfigEvent::GenieCodeClear);
            self.add_message(
                MessageType::Info,
                "Cleared Game Genie codes entered for the previous game",
            );
        }
        if self.auto_load {
            let save_path = Config::save_path(&rom.name, self.save_slot);
            if let Err(err) = self.control_deck.load_state_path(save_path)
                && !matches!(err, control_deck::Error::NoSaveStateFound)
            {
                error!("failed to load state: {err:?}");
            }
        }
        if let Err(err) = self.audio.start() {
            self.tx.event(ConfigEvent::AudioEnabled(false));
            self.on_error(err);
        }
        self.tx.event(RendererEvent::RomLoaded(rom));
        self.tx.event(RendererEvent::RequestRedraw {
            viewport_id: ViewportId::ROOT,
            when: Instant::now(),
        });
        self.reset_frame_stats();
        self.last_auto_save = Instant::now();
    }

    fn load_rom_path(&mut self, path: impl AsRef<std::path::Path>) {
        let path = path.as_ref();
        self.unload_rom();
        match self.control_deck.load_rom_path(path) {
            Ok(rom) => self.on_load_rom(rom),
            Err(err) => self.on_error(err),
        }
    }

    fn load_rom(&mut self, name: &str, rom: &mut impl Read) {
        self.unload_rom();
        match self.control_deck.load_rom(name, rom) {
            Ok(rom) => self.on_load_rom(rom),
            Err(err) => self.on_error(err),
        }
    }

    fn on_load_replay(&mut self, start: Bus, name: impl AsRef<str>) {
        self.add_message(
            MessageType::Info,
            format!("Loaded Replay Recording {:?}", name.as_ref()),
        );
        if let Err(err) = self.control_deck.load_bus(start) {
            self.on_error(anyhow::Error::from(err));
            return;
        }
        self.tx.event(RendererEvent::ReplayLoaded);
        self.tx.event(RendererEvent::RequestRedraw {
            viewport_id: ViewportId::ROOT,
            when: Instant::now(),
        });
    }

    fn load_replay_path(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        match self.replay.load_path(path) {
            Ok(start) => self.on_load_replay(start, path.to_string_lossy()),
            Err(err) => self.on_error(err),
        }
    }

    fn load_replay(&mut self, name: &str, replay: &mut impl Read) {
        match self.replay.load(replay) {
            Ok(start) => self.on_load_replay(start, name),
            Err(err) => self.on_error(err),
        }
    }

    fn update_region(&mut self, region: NesRegion) {
        self.target_frame_duration = FrameRate::from(region).duration();
        self.frame_latency = (self.audio.latency.as_secs_f32()
            / self.target_frame_duration.as_secs_f32())
        .ceil() as usize;
    }

    fn audio_record(&mut self, recording: bool) {
        if self.control_deck.is_running() {
            if !recording && self.audio.is_recording() {
                match self.audio.stop_recording() {
                    Ok(Some(filename)) => {
                        self.add_message(
                            MessageType::Info,
                            format!("Saved Replay Recording {filename:?}"),
                        );
                    }
                    Err(err) => self.on_error(err),
                    _ => (),
                }
            } else if recording && let Err(err) = self.audio.start_recording() {
                self.on_error(err);
            }
        }
    }

    fn replay_record(&mut self, recording: bool) {
        if self.control_deck.is_running() {
            if recording {
                self.record.start(self.control_deck.bus().clone());
            } else if let Some(rom) = self.control_deck.loaded_rom() {
                match self.record.stop(&rom.name) {
                    Ok(Some(filename)) => {
                        self.add_message(
                            MessageType::Info,
                            format!("Saved Replay Recording {filename:?}"),
                        );
                    }
                    Err(err) => self.on_error(err),
                    _ => (),
                }
            }
        }
    }

    fn save_screenshot(&mut self) -> anyhow::Result<PathBuf> {
        let picture_dir = Config::default_picture_dir();
        let filename = picture_dir
            .join(
                Local::now()
                    .format("screenshot_%Y-%m-%d_at_%H_%M_%S")
                    .to_string(),
            )
            .with_extension("png");
        let image = image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
            u32::from(ppu::size::WIDTH),
            u32::from(ppu::size::HEIGHT),
            self.control_deck.frame_buffer(),
        )
        .ok_or_else(|| anyhow!("failed to create image buffer"))?;

        if !picture_dir.exists() {
            std::fs::create_dir_all(&picture_dir)
                .with_context(|| format!("failed to create screenshot dir: {picture_dir:?}"))?;
        }

        // TODO: provide wasm download
        image
            .save(&filename)
            .map(|_| filename.clone())
            .with_context(|| format!("failed to save screenshot: {filename:?}"))
    }

    /// Dynamic rate control: hold the audio buffer at half full by nudging the output rate.
    ///
    /// The emulator clocks frames on the wall clock and the sound card consumes on its own, and
    /// the two never agree exactly - a 60.0988 Hz console, a 60 Hz-ish display and a 48 kHz-ish
    /// card all have their own tolerances. Left alone the audio buffer drifts to one end and
    /// either underruns or forces the emulator to wait, and waiting only trades the audio glitch
    /// for a video one.
    ///
    /// Instead, ask the APU for slightly more or fewer samples per frame:
    ///
    /// ```text
    /// ratio = 1 + d * (1 - 2 * level)
    /// ```
    ///
    /// where `level` is how full the buffer is, 0.0 to 1.0. Below half full the ratio rises and
    /// the buffer fills; above half it falls and the buffer drains, so the buffer converges on
    /// half full - the point with the most room either side for jitter. The buffer is sized at
    /// twice the configured latency, so half full *is* the latency the user asked for.
    ///
    /// From Arntzen, "Dynamic Rate Control for Retro Game Emulators" (2012), which is what
    /// RetroArch implements. The pitch shift this costs is bounded by `d` and in practice runs an
    /// order of magnitude under it - the paper measures 0.062% deviation for `d = 0.005` - which
    /// is comfortably below both the audible threshold and the tolerance of the card's own
    /// oscillator.
    fn update_audio_rate(&mut self) {
        // `None` while paused or stopped, when nothing is being queued and the level means
        // nothing. Leave the ratio where it is rather than winding it to an extreme.
        let Some(level) = self.audio.buffer_level() else {
            return;
        };

        // Track out whatever the nominal rate is persistently wrong by - the paper's section 2.5,
        // "updating ratio estimate". The proportional term alone can only hold a standing error by
        // sitting away from its setpoint, and a buffer parked away from half full has less cushion
        // than the user asked for on the side it has drifted toward.
        //
        // There is always a standing error to absorb: the frame rate paced on here is a rounded
        // integer, 60 rather than the NTSC console's 60.0988, which is 0.2% before the display's
        // real refresh and the card's real sample rate are counted.
        self.audio_rate_bias = (self.audio_rate_bias + Self::AUDIO_BIAS_GAIN * (0.5 - level))
            .clamp(1.0 - Self::AUDIO_MAX_BIAS, 1.0 + Self::AUDIO_MAX_BIAS);

        let ratio = self.audio_rate_bias * Self::audio_rate_ratio(level);
        trace!(
            "audio buffer {level:.3} full, bias {:.5}, rate ratio {ratio:.5}",
            self.audio_rate_bias
        );
        self.control_deck.set_audio_sample_ratio(ratio);
    }

    /// The output-rate multiplier that steers a buffer at `level` back toward half full.
    ///
    /// Split out from [`State::update_audio_rate`] so the control law can be tested without an
    /// audio device.
    fn audio_rate_ratio(level: f32) -> f32 {
        1.0 + Self::AUDIO_MAX_DEVIATION * (1.0 - 2.0 * level)
    }

    fn park_duration(&self) -> Option<Duration> {
        let park_epsilon = Duration::from_millis(1);
        // Park if we're paused, occluded, or not running
        let duration = if self.run_state.paused() || !self.control_deck.is_running() {
            Some(self.target_frame_duration - park_epsilon)
        } else if self.bench.is_some() {
            // Benchmarking measures how fast frames *can* be produced, so the frame clock that
            // normally holds them to the region's rate does not apply. Only the running check
            // above still does: a paused benchmark would spin without producing frames.
            None
        } else {
            // A steady wall clock, whatever audio is doing. `update_audio_rate` holds the audio
            // queue at its target by bending pitch imperceptibly; gating frame timing on that
            // queue instead lets the sound card's buffer quantum decide when frames appear, and
            // leaves an empty queue meaning no rate limit at all.
            let now = Instant::now();
            (now < self.next_frame_time).then(|| self.next_frame_time - now)
        };
        duration.map(|duration| {
            // Parking thread is only required for Multi-threaded emulation to save CPU cycles.
            if self.threaded {
                duration
            } else {
                Duration::ZERO
            }
        })
    }

    /// Clocks the NES frames this display frame owes, snapshotting each one for rewind, and
    /// reports the breakpoint it stopped at if it reached one.
    ///
    /// The emulation speed decides how many that is - two at 2x, none every other display frame at
    /// 0.5x - and taking them one at a time is what keeps the rewind buffer evenly spaced in game
    /// time. Recording per display frame instead made a fast-forwarded stretch rewind at the speed
    /// it was recorded at.
    fn clock_display_frame(&mut self) -> control_deck::Result<Option<u16>> {
        loop {
            let clocked = if self.debug_breakpoints.is_empty() {
                self.control_deck.clock_frame()?
            } else {
                let breakpoints = &self.debug_breakpoints;
                self.control_deck.clock_frame_until(|bus| {
                    bus.access_hit.is_some()
                        || breakpoints.iter().any(|breakpoint| {
                            breakpoint.breaks && breakpoint.covers(bus.cpu.pc, Access::EXEC)
                        })
                })?
            };
            if clocked == Clocked::Stopped {
                // Part way through a frame, so there is nothing to snapshot: the frame the display
                // frame owes has not been clocked, and resuming finishes it.
                return Ok(Some(self.control_deck.bus().cpu.pc));
            }
            if clocked != Clocked::Idle
                && let Err(err) = self.rewind.push(&self.control_deck)
            {
                self.rewind.set_enabled(false);
                self.on_error(err);
            }
            if clocked != Clocked::Continue {
                return Ok(None);
            }
        }
    }

    /// Clock a display frame if one is due, or report how long until the next one is.
    ///
    /// `Some(duration)` means nothing was clocked and there is that long to wait; the caller
    /// decides how, since only it knows what else it might be woken for.
    fn try_clock_frame(&mut self) -> Option<Duration> {
        // If any frames are still pending, ask the renderer again - but at most once a frame.
        //
        // This is a retry, not the request: the request itself goes out when a frame is produced,
        // at the bottom of this function. Each retry is a cross-thread event-loop wakeup and this
        // loop can iterate far faster than a frame, so unthrottled the two threads wake each other
        // in a loop and neither gets on with its work.
        if !self.frame_tx.is_empty()
            && self.last_redraw_request.elapsed() >= self.target_frame_duration
        {
            self.request_redraw();
        }

        if let Some(wait) = self.park_duration() {
            return Some(wait);
        }

        if self.rewinding {
            // Stop rewinding if a restore fails
            if self.rewind.pop(&mut self.control_deck) {
                // A rewind frame excludes the frame buffer, so render it by clocking the frame
                // Below 1x speed, clocking may not produce a frame so clock until one is rendered.
                loop {
                    match self.control_deck.clock_frame() {
                        Ok(Clocked::Idle) => continue,
                        Ok(_) => break,
                        Err(err) => {
                            error!("failed to render a rewound frame: {err:?}");
                            self.set_rewinding(false);
                            return None;
                        }
                    }
                }
                self.send_frame();
                self.update_frame_stats();
            } else {
                self.set_rewinding(false);
            }
        } else {
            if let Some(event) = self.replay.next(self.control_deck.frame_number()) {
                self.on_emulation_event(&event);
            }

            match self.clock_display_frame() {
                Ok(Some(addr)) => self.on_breakpoint(addr),
                Ok(None) => {
                    self.audio.process(self.control_deck.audio_samples());
                    self.update_audio_rate();
                    self.send_frame();
                    self.update_frame_stats();
                    self.update_bench();
                    if self.auto_save && self.last_auto_save.elapsed() > self.auto_save_interval {
                        self.last_auto_save = Instant::now();
                        self.save_state(self.save_slot, true);
                    }
                }
                Err(err) => self.on_error(err),
            }
        }

        self.advance_deadline();
        // Request to draw this frame
        self.request_redraw();
        None
    }

    /// Move the frame deadline on by one, resynchronising if it is already far in the past.
    ///
    /// Frames owed after a stall are worth repaying up to a point: they are audio the buffer is
    /// short of, and rate control can only refill it by half a percent a frame. Past that point
    /// repaying becomes a sprint through everything missed, so the debt is written off and the
    /// clock restarts from now.
    fn advance_deadline(&mut self) {
        self.next_frame_time += self.target_frame_duration;
        let now = Instant::now();
        if now.saturating_duration_since(self.next_frame_time)
            > Self::MAX_CATCHUP_FRAMES * self.target_frame_duration
        {
            self.next_frame_time = now + self.target_frame_duration;
        }
    }

    fn request_redraw(&mut self) {
        self.last_redraw_request = Instant::now();
        self.tx.event(RendererEvent::RequestRedraw {
            viewport_id: ViewportId::ROOT,
            when: Instant::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulate the closed loop: each frame the emulator pushes `produced * ratio` samples and the
    /// sound card takes `consumed`, both as a fraction of buffer capacity.
    ///
    /// Returns the buffer level over time, starting from `start`.
    fn simulate(start: f32, produced: f32, consumed: f32, frames: usize) -> Vec<f32> {
        let mut level = start;
        let mut bias = 1.0f32;
        let mut levels = Vec::with_capacity(frames);
        for _ in 0..frames {
            level += produced * bias * State::audio_rate_ratio(level) - consumed;
            level = level.clamp(0.0, 1.0);
            bias = (bias + State::AUDIO_BIAS_GAIN * (0.5 - level))
                .clamp(1.0 - State::AUDIO_MAX_BIAS, 1.0 + State::AUDIO_MAX_BIAS);
            levels.push(level);
        }
        levels
    }

    /// The control law's setpoint is a half-full buffer, which is where the configured latency
    /// sits and where there is the most room either side for jitter.
    #[test]
    fn the_audio_rate_ratio_steers_toward_a_half_full_buffer() {
        assert_eq!(
            State::audio_rate_ratio(0.5),
            1.0,
            "a half-full buffer needs no correction"
        );
        assert!(
            State::audio_rate_ratio(0.0) > 1.0,
            "an empty buffer must ask for more samples"
        );
        assert!(
            State::audio_rate_ratio(1.0) < 1.0,
            "a full buffer must ask for fewer"
        );

        // Monotonic, so there is exactly one setpoint and no oscillation between two.
        let mut previous = f32::INFINITY;
        for step in 0..=100 {
            let ratio = State::audio_rate_ratio(step as f32 / 100.0);
            assert!(ratio < previous, "must fall with level, at {step}");
            previous = ratio;
        }
    }

    /// The pitch shift is what this method costs, and the whole argument for it is that the cost
    /// is inaudible. Bound it explicitly rather than trusting the constant not to be edited.
    #[test]
    fn the_audio_rate_ratio_never_bends_pitch_audibly() {
        for step in 0..=100 {
            let deviation = (State::audio_rate_ratio(step as f32 / 100.0) - 1.0).abs();
            assert!(
                deviation <= State::AUDIO_MAX_DEVIATION,
                "level {step} bends pitch by {deviation}"
            );
        }
        // A semitone is ~5.9%, and the smallest interval anyone reliably hears is far above 0.5%.
        const { assert!(State::AUDIO_MAX_DEVIATION <= 0.005) };
    }

    /// A buffer starting far from the setpoint has to come back to it and stay, or the method
    /// buys nothing: the point is that the buffer neither underruns nor fills.
    #[test]
    fn a_drained_or_flooded_buffer_converges() {
        // One frame of a 60 Hz console against a 100 ms buffer is ~1/6th of capacity.
        let rate = 1.0 / 6.0;
        for start in [0.0, 0.1, 0.5, 0.9, 1.0] {
            let levels = simulate(start, rate, rate, 20_000);
            let settled = levels[levels.len() - 1];
            assert!(
                (settled - 0.5).abs() < 0.01,
                "started at {start}, settled at {settled}"
            );
            // And without ever hitting an end on the way, which is what an underrun sounds like.
            let lowest = levels.iter().copied().fold(f32::INFINITY, f32::min);
            assert!(lowest > 0.0, "started at {start}, drained to {lowest}");
        }
    }

    /// A nominal rate that is persistently wrong - a rounded frame rate, a sound card that is not
    /// quite at its stated rate - is what the base-rate estimate exists for. The buffer has to end
    /// up back at half full, not merely somewhere safe: sitting off-center is sitting with less
    /// cushion than the user asked for, and that is what a hitch then falls through.
    #[test]
    fn a_persistent_rate_error_is_tracked_out_rather_than_absorbed_by_the_buffer() {
        let rate = 1.0 / 6.0;
        // Spanning the real one measured on this machine (~0.27%, from pacing a 60.0988 Hz
        // console at a rounded 60 fps) in both directions.
        for drift in [1.004, 1.002, 1.0, 0.998, 0.996] {
            for start in [0.0, 0.5, 1.0] {
                let levels = simulate(start, rate, rate * drift, 40_000);
                let settled = levels[levels.len() - 1];
                assert!(
                    (settled - 0.5).abs() < 0.02,
                    "drift {drift} from {start} settled at {settled}"
                );
                let highest = levels.iter().copied().fold(0.0f32, f32::max);
                assert!(
                    highest < 1.0,
                    "drift {drift} from {start} filled the buffer"
                );
            }
        }
    }

    /// The estimate must not mistake a dropout for a rate error. A hitch is transient and the
    /// buffer refills on its own; winding the base rate out to its limit every time one happened
    /// would leave the pitch bent long after the cause had gone.
    #[test]
    fn a_transient_dropout_barely_moves_the_base_rate_estimate() {
        // Sixty frames - a full second - of the buffer sitting far below its setpoint.
        let mut bias = 1.0f32;
        for _ in 0..60 {
            bias = (bias + State::AUDIO_BIAS_GAIN * (0.5 - 0.05))
                .clamp(1.0 - State::AUDIO_MAX_BIAS, 1.0 + State::AUDIO_MAX_BIAS);
        }
        let moved = (bias - 1.0).abs();
        assert!(
            moved < State::AUDIO_MAX_BIAS / 4.0,
            "a one-second dropout moved the estimate by {moved}, most of its {} range",
            State::AUDIO_MAX_BIAS
        );
    }
}
