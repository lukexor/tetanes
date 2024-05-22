use crate::{
    nes::{
        action::DebugStep,
        audio::{Audio, State as AudioState},
        config::{Config, FrameRate},
        emulation::{replay::Record, rewind::Rewind},
        event::{ConfigEvent, EmulationEvent, NesEvent, RendererEvent, SendNesEvent, UiEvent},
        renderer::{gui::MessageType, FrameRecycle},
    },
    thread,
};
use anyhow::{anyhow, bail};
use chrono::Local;
use crossbeam::channel;
use egui::ViewportId;
use replay::Replay;
use std::{
    collections::VecDeque,
    io::{self, Read},
    path::{Path, PathBuf},
    thread::JoinHandle,
};
use tetanes_core::{
    apu::Apu,
    common::{NesRegion, Regional, Reset, ResetKind},
    control_deck::{self, ControlDeck, LoadedRom},
    cpu::Cpu,
    ppu::Ppu,
    time::{Duration, Instant},
    video::Frame,
};
use thingbuf::mpsc::{blocking::Sender as BufSender, errors::TrySendError};
use tracing::{debug, error};
use winit::{event::ElementState, event_loop::EventLoopProxy};

pub mod replay;
pub mod rewind;

#[derive(Default, Debug, Copy, Clone, PartialEq)]
#[must_use]
pub struct FrameStats {
    pub fps: f32,
    pub fps_min: f32,
    pub frame_time: f32,
    pub frame_time_max: f32,
    pub frame_count: usize,
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
    history: VecDeque<f32>,
    sum: f32,
    avg: f32,
    last_update: Instant,
}

impl FrameTimeDiag {
    const MAX_HISTORY: usize = 120;
    const UPDATE_INTERVAL: Duration = Duration::from_millis(300);

    fn new() -> Self {
        Self {
            frame_count: 0,
            history: VecDeque::with_capacity(Self::MAX_HISTORY),
            sum: 0.0,
            avg: 1.0 / 60.0,
            last_update: Instant::now(),
        }
    }

    fn push(&mut self, frame_time: f32) {
        self.frame_count += 1;

        // Ignore the first few frames to allow the average to stabilize
        if frame_time.is_finite() && self.frame_count >= 10 {
            if self.history.len() >= Self::MAX_HISTORY {
                if let Some(oldest) = self.history.pop_front() {
                    self.sum -= oldest;
                }
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

    fn reset(&mut self) {
        self.frame_count = 0;
        self.history.clear();
        self.sum = 0.0;
        self.avg = 1.0 / 60.0;
        self.last_update = Instant::now();
    }
}

fn shutdown(tx: &EventLoopProxy<NesEvent>, err: impl std::fmt::Display) {
    error!("{err}");
    tx.nes_event(UiEvent::Terminate);
    std::process::exit(1);
}

#[derive(Debug)]
#[must_use]
enum Threads {
    Single(Single),
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
    handle: JoinHandle<()>,
}

impl Multi {
    fn spawn(
        proxy_tx: EventLoopProxy<NesEvent>,
        frame_tx: BufSender<Frame, FrameRecycle>,
        config: Config,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = channel::bounded(1024);
        Ok(Self {
            tx,
            handle: std::thread::Builder::new()
                .name("emulation".into())
                .spawn(move || Self::main(proxy_tx, rx, frame_tx, config))?,
        })
    }

    fn main(
        tx: EventLoopProxy<NesEvent>,
        rx: channel::Receiver<NesEvent>,
        frame_tx: BufSender<Frame, FrameRecycle>,
        config: Config,
    ) {
        debug!("emulation thread started");
        let mut state = State::new(tx, frame_tx, config); // Has to be created on the thread, since
        loop {
            #[cfg(feature = "profiling")]
            puffin::profile_scope!("emulation loop");

            while let Ok(event) = rx.try_recv() {
                state.on_event(&event);
            }

            state.clock_frame();
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
        tx: EventLoopProxy<NesEvent>,
        frame_tx: BufSender<Frame, FrameRecycle>,
        cfg: Config,
    ) -> anyhow::Result<Self> {
        let threaded = cfg.emulation.threaded
            && std::thread::available_parallelism().map_or(false, |count| count.get() > 1);
        let backend = if threaded {
            Threads::Multi(Multi::spawn(tx, frame_tx, cfg)?)
        } else {
            Threads::Single(Single {
                state: State::new(tx, frame_tx, cfg),
            })
        };

        Ok(Self { threads: backend })
    }

    /// Handle event.
    pub fn on_event(&mut self, event: &NesEvent) {
        match &mut self.threads {
            Threads::Single(Single { state }) => state.on_event(event),
            Threads::Multi(Multi { tx, handle }) => {
                handle.thread().unpark();
                if let Err(err) = tx.try_send(event.clone()) {
                    error!("failed to send emulation event: {event:?}. {err:?}");
                    std::process::exit(1);
                }
            }
        }
    }

    pub fn clock_frame(&mut self) {
        match &mut self.threads {
            Threads::Single(Single { state }) => state.clock_frame(),
            // Multi-threaded emulation handles it's own clock timing and redraw requests
            Threads::Multi(Multi { handle, .. }) => handle.thread().unpark(),
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct State {
    tx: EventLoopProxy<NesEvent>,
    control_deck: ControlDeck,
    audio: Audio,
    frame_tx: BufSender<Frame, FrameRecycle>,
    frame_latency: usize,
    target_frame_duration: Duration,
    last_clock_time: Instant,
    clock_time_accumulator: f32,
    last_frame_time: Instant,
    frame_time_diag: FrameTimeDiag,
    unfocused_paused: bool,
    paused: bool,
    rewinding: bool,
    rewind: Rewind,
    record: Record,
    replay: Replay,
    save_slot: u8,
    auto_save: bool,
    auto_save_interval: Duration,
    last_auto_save: Instant,
    auto_load: bool,
    speed: f32,
    run_ahead: usize,
    show_frame_stats: bool,
}

impl Drop for State {
    fn drop(&mut self) {
        self.unload_rom();
    }
}

impl State {
    fn new(
        tx: EventLoopProxy<NesEvent>,
        frame_tx: BufSender<Frame, FrameRecycle>,
        cfg: Config,
    ) -> Self {
        let mut control_deck = ControlDeck::with_config(cfg.deck.clone());
        let audio = Audio::new(
            cfg.audio.enabled,
            Apu::DEFAULT_SAMPLE_RATE,
            cfg.audio.latency,
            cfg.audio.buffer_size,
        );
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
            last_clock_time: Instant::now(),
            clock_time_accumulator: 0.0,
            last_frame_time: Instant::now(),
            frame_time_diag: FrameTimeDiag::new(),
            unfocused_paused: false,
            paused: true,
            rewinding: false,
            rewind,
            record: Record::new(),
            replay: Replay::new(),
            save_slot: cfg.emulation.save_slot,
            auto_save: cfg.emulation.auto_save,
            auto_save_interval: cfg.emulation.auto_save_interval,
            last_auto_save: Instant::now(),
            auto_load: cfg.emulation.auto_load,
            speed: cfg.emulation.speed,
            run_ahead: cfg.emulation.run_ahead,
            show_frame_stats: false,
        };
        state.update_region(cfg.deck.region);
        state
    }

    pub(crate) fn add_message<S: ToString>(&mut self, ty: MessageType, msg: S) {
        self.tx.nes_event(UiEvent::Message((ty, msg.to_string())));
    }

    fn write_deck<T>(
        &mut self,
        writer: impl FnOnce(&mut ControlDeck) -> control_deck::Result<T>,
    ) -> Option<T> {
        writer(&mut self.control_deck)
            .map_err(|err| {
                self.pause(true);
                self.on_error(err);
            })
            .ok()
    }

    fn on_error(&mut self, err: impl Into<anyhow::Error>) {
        let err = err.into();
        error!("Emulation error: {err:?}");
        self.add_message(MessageType::Error, err);
    }

    /// Handle event.
    fn on_event(&mut self, event: &NesEvent) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        match event {
            NesEvent::Emulation(event) => self.on_emulation_event(event),
            NesEvent::Config(event) => self.on_config_event(event),
            _ => (),
        }
    }

    /// Handle emulation event.
    fn on_emulation_event(&mut self, event: &EmulationEvent) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        match event {
            EmulationEvent::AudioRecord(recording) => {
                if self.control_deck.is_running() {
                    self.audio_record(*recording);
                }
            }
            EmulationEvent::DebugStep(step) => {
                if self.control_deck.is_running() {
                    match step {
                        DebugStep::Into => {
                            self.write_deck(|deck| deck.clock_instr());
                            self.send_frame();
                        }
                        DebugStep::Out => {
                            // TODO: track stack frames list on jsr, irq, brk
                            // while stack frame == previous stack frame, clock_instr, send_frame
                            self.send_frame();
                        }
                        DebugStep::Over => {
                            // TODO: track stack frames list on jsr, irq, brk
                            // while stack frame != previous stack frame, clock_instr, send_frame
                            self.send_frame();
                        }
                        DebugStep::Scanline => {
                            if self.write_deck(|deck| deck.clock_scanline()).is_some() {
                                self.send_frame();
                            }
                        }
                        DebugStep::Frame => {
                            if self.write_deck(|deck| deck.clock_frame()).is_some() {
                                self.send_frame();
                            }
                        }
                    }
                }
            }
            EmulationEvent::EmulatePpuWarmup(enabled) => {
                self.control_deck.set_emulate_ppu_warmup(*enabled);
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
            EmulationEvent::Pause(paused) => {
                if self.control_deck.is_running() {
                    self.pause(*paused);
                }
            }
            EmulationEvent::ReplayRecord(recording) => {
                if self.control_deck.is_running() {
                    self.replay_record(*recording);
                }
            }
            EmulationEvent::Reset(kind) => {
                self.frame_time_diag.reset();
                if self.control_deck.is_running() {
                    self.control_deck.reset(*kind);
                    self.pause(false);
                    match kind {
                        ResetKind::Soft => self.add_message(MessageType::Info, "Reset"),
                        ResetKind::Hard => self.add_message(MessageType::Info, "Power Cycled"),
                    }
                }
            }
            EmulationEvent::Rewinding(rewind) => {
                if self.control_deck.is_running() {
                    if self.rewind.enabled {
                        self.rewinding = *rewind;
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
                self.frame_time_diag.reset();
                self.show_frame_stats = *show;
            }
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
            EmulationEvent::UnfocusedPause(paused) => {
                self.unfocused_paused = *paused;
                if self.control_deck.is_running() {
                    self.audio.pause(self.unfocused_paused);
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
                self.control_deck
                    .set_apu_channel_enabled(*channel, *enabled);
                let enabled_text = if *enabled { "Enabled" } else { "Disabled" };
                self.add_message(
                    MessageType::Info,
                    format!("{enabled_text} APU Channel {channel:?}"),
                );
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
            ConfigEvent::CycleAccurate(enabled) => {
                self.control_deck.set_cycle_accurate(*enabled);
            }
            ConfigEvent::FourPlayer(four_player) => {
                self.control_deck.set_four_player(*four_player);
            }
            ConfigEvent::GenieCodeAdded(genie_code) => {
                self.control_deck
                    .cpu_mut()
                    .bus
                    .add_genie_code(genie_code.clone());
            }
            ConfigEvent::GenieCodeRemoved(code) => {
                self.control_deck.remove_genie_code(code);
            }
            ConfigEvent::RamState(ram_state) => {
                self.control_deck.set_ram_state(*ram_state);
            }
            ConfigEvent::Region(region) => {
                self.control_deck.set_region(*region);
                self.update_region(*region);
            }
            ConfigEvent::RewindEnabled(enabled) => self.rewind.set_enabled(*enabled),
            ConfigEvent::RewindSeconds(seconds) => self.rewind.set_seconds(*seconds),
            ConfigEvent::RewindInterval(interval) => self.rewind.set_interval(*interval),
            ConfigEvent::RunAhead(run_ahead) => self.run_ahead = *run_ahead,
            ConfigEvent::SaveSlot(slot) => self.save_slot = *slot,
            ConfigEvent::MapperRevisions(revs) => {
                self.control_deck.set_mapper_revisions(*revs);
            }
            ConfigEvent::Speed(speed) => {
                self.speed = *speed;
                self.control_deck.set_frame_speed(*speed);
            }
            ConfigEvent::VideoFilter(filter) => self.control_deck.set_filter(*filter),
            ConfigEvent::ZapperConnected(connected) => {
                self.control_deck.connect_zapper(*connected);
            }
            ConfigEvent::HideOverscan(_) | ConfigEvent::InputBindings => (),
        }
    }

    fn update_frame_stats(&mut self) {
        if !self.show_frame_stats {
            return;
        }

        self.frame_time_diag
            .push(self.last_frame_time.elapsed().as_secs_f32());
        self.last_frame_time = Instant::now();
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
        self.tx.nes_event(RendererEvent::FrameStats(FrameStats {
            fps,
            fps_min,
            frame_time: frame_time * 1000.0,
            frame_time_max: frame_time_max * 1000.0,
            frame_count: self.frame_time_diag.frame_count,
        }));
    }

    fn send_frame(&mut self) {
        // Indicate we want to redraw to ensure there's a frame slot made available if
        // the pool is already full
        self.tx.nes_event(RendererEvent::RequestRedraw {
            viewport_id: ViewportId::ROOT,
            when: Instant::now(),
        });
        // IMPORTANT: Wasm can't block
        if self.audio.enabled() || cfg!(target_arch = "wasm32") {
            match self.frame_tx.try_send_ref() {
                Ok(mut frame) => self.control_deck.frame_buffer_into(&mut frame),
                Err(TrySendError::Full(_)) => debug!("dropped frame"),
                Err(_) => shutdown(&self.tx, "failed to get frame"),
            }
        } else if let Ok(mut frame) = self.frame_tx.send_ref() {
            self.control_deck.frame_buffer_into(&mut frame);
        }
    }

    fn pause(&mut self, paused: bool) {
        if !self.control_deck.cpu_corrupted() {
            self.paused = paused;
            if self.paused {
                if let Some(rom) = self.control_deck.loaded_rom() {
                    if let Err(err) = self.record.stop(&rom.name) {
                        self.on_error(err);
                    }
                }
            }
            self.audio.pause(self.paused);
            if !self.paused {
                self.last_auto_save = Instant::now();
                // To avoid having a large dip in frame stats when unpausing
                self.last_frame_time = Instant::now();
            }
        } else {
            self.paused = true;
        }
    }

    fn save_state(&mut self, slot: u8, auto: bool) {
        if let Some(rom) = self.control_deck.loaded_rom() {
            if let Some(data_dir) = Config::save_path(&rom.name, slot) {
                match self.control_deck.save_state(data_dir) {
                    Ok(_) => {
                        if !auto {
                            self.add_message(MessageType::Info, format!("State {slot} Saved"));
                        }
                    }
                    Err(err) => self.on_error(err),
                }
            }
        }
    }

    fn load_state(&mut self, slot: u8) {
        if let Some(rom) = self.control_deck.loaded_rom() {
            if let Some(path) = Config::save_path(&rom.name, slot) {
                match self.control_deck.load_state(path) {
                    Ok(_) => self.add_message(MessageType::Info, format!("State {slot} Loaded")),
                    Err(err) => self.on_error(err),
                }
            }
        }
    }

    fn unload_rom(&mut self) {
        if let Some(rom) = self.control_deck.loaded_rom() {
            if self.auto_save {
                if let Some(path) = Config::save_path(&rom.name, self.save_slot) {
                    if let Err(err) = self.control_deck.save_state(path) {
                        self.on_error(err);
                    }
                }
            }
            self.replay_record(false);
            self.rewind.clear();
            let _ = self.audio.stop();
            if let Err(err) = self.control_deck.unload_rom() {
                self.on_error(err);
            }
            self.tx.nes_event(RendererEvent::RomUnloaded);
            self.frame_time_diag.reset();
        }
    }

    fn on_load_rom(&mut self, rom: LoadedRom) {
        if self.auto_load {
            if let Some(path) = Config::save_path(&rom.name, self.save_slot) {
                if let Err(err) = self.control_deck.load_state(path) {
                    error!("failed to load state: {err:?}");
                }
            }
        }
        self.tx.nes_event(RendererEvent::RomLoaded(rom));
        if let Err(err) = self.audio.start() {
            self.on_error(err);
        }
        self.pause(false);
        self.frame_time_diag.reset();
        self.last_auto_save = Instant::now();
        // To avoid having a large dip in frame stats after loading
        self.last_frame_time = Instant::now();
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

    fn on_load_replay(&mut self, start: Cpu, name: impl AsRef<str>) {
        self.add_message(
            MessageType::Info,
            format!("Loaded Replay Recording {:?}", name.as_ref()),
        );
        self.control_deck.load_cpu(start);
        self.pause(false);
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
            } else if recording {
                if let Err(err) = self.audio.start_recording() {
                    self.on_error(err);
                }
            }
        }
    }

    fn replay_record(&mut self, recording: bool) {
        if self.control_deck.is_running() {
            if recording {
                self.record.start(self.control_deck.cpu().clone());
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
        match Config::default_picture_dir() {
            Some(picture_dir) => {
                let filename = picture_dir
                    .join(
                        Local::now()
                            .format("screenshot_%Y-%m-%d_at_%H_%M_%S")
                            .to_string(),
                    )
                    .with_extension("png");
                let image = image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
                    Ppu::WIDTH,
                    Ppu::HEIGHT,
                    self.control_deck.frame_buffer(),
                )
                .ok_or_else(|| anyhow!("failed to create image buffer"))?;

                // TODO: provide wasm download
                Ok(image.save(&filename).map(|_| filename)?)
            }
            None => bail!("failed to find default picture directory"),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn should_park(&self) -> bool {
        if self.audio.enabled() {
            self.audio.queued_time() >= self.audio.latency
        } else {
            self.clock_time_accumulator < self.target_frame_duration.as_secs_f32()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn should_park(&self) -> bool {
        self.audio.enabled() && self.audio.queued_time() >= self.audio.latency
    }

    fn clock_frame(&mut self) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let last_clock_duration = self.last_clock_time.elapsed();
        self.last_clock_time = Instant::now();
        let frame_duration_secs = last_clock_duration.as_secs_f32();
        self.clock_time_accumulator += frame_duration_secs;
        if self.clock_time_accumulator > 0.020 {
            self.clock_time_accumulator = 0.020;
        }

        let park_epsilon = Duration::from_millis(1);
        // Park if we're paused, occluded, or not running
        if self.paused || self.unfocused_paused || !self.control_deck.is_running() {
            // But if we're only running + paused and not occluded, send a frame
            if self.paused && !self.unfocused_paused && self.control_deck.is_running() {
                self.send_frame();
            }
            thread::park_timeout(self.target_frame_duration - park_epsilon);
            return;
        }
        if !self.rewinding && self.should_park() {
            thread::park_timeout(self.audio.queued_time().saturating_sub(self.audio.latency));
            return;
        }

        // Clock frames until we catch up to the audio queue latency as long as audio is enabled and we're
        // not rewinding, otherwise fall back to time-based clocking
        // let mut clocked_frames = 0; // Prevent infinite loop when queued audio falls behind
        let mut run_ahead = self.run_ahead;
        if self.speed > 1.0 {
            run_ahead = 0;
        }

        if self.rewinding {
            match self.rewind.pop() {
                Some(cpu) => {
                    self.control_deck.load_cpu(cpu);
                    self.send_frame();
                    self.update_frame_stats();
                    thread::park_timeout(self.target_frame_duration - park_epsilon);
                }
                None => self.rewinding = false,
            }
        } else {
            if let Some(event) = self.replay.next(self.control_deck.frame_number()) {
                self.on_emulation_event(&event);
            }
            let res = self.control_deck.clock_frame_ahead(
                run_ahead,
                |_cycles, frame_buffer, audio_samples| {
                    self.audio.process(audio_samples);
                    let send_frame = |frame: &mut Frame| {
                        frame.clear();
                        frame.extend_from_slice(frame_buffer);
                    };
                    self.clock_time_accumulator -= frame_duration_secs;

                    // Indicate we want to redraw to ensure there's a frame slot made available if
                    // the pool is already full
                    self.tx.nes_event(RendererEvent::RequestRedraw {
                        viewport_id: ViewportId::ROOT,
                        when: Instant::now(),
                    });
                    // IMPORTANT: Wasm can't block
                    if self.audio.enabled() || cfg!(target_arch = "wasm32") {
                        // If audio is enabled or wasm, frame rate is controlled by park_timeout
                        // above
                        match self.frame_tx.try_send_ref() {
                            Ok(mut frame) => send_frame(&mut frame),
                            Err(TrySendError::Full(_)) => debug!("dropped frame"),
                            Err(_) => shutdown(&self.tx, "failed to get frame"),
                        }
                    } else {
                        // Otherwise we'll block on vsync
                        match self.frame_tx.send_ref() {
                            Ok(mut frame) => send_frame(&mut frame),
                            Err(_) => shutdown(&self.tx, "failed to get frame"),
                        }
                    }
                },
            );
            match res {
                Ok(()) => {
                    self.update_frame_stats();
                    if let Err(err) = self.rewind.push(self.control_deck.cpu()) {
                        self.rewind.set_enabled(false);
                        self.on_error(err);
                    }
                    if self.last_auto_save.elapsed() > self.auto_save_interval {
                        self.last_auto_save = Instant::now();
                        self.save_state(self.save_slot, true);
                    }
                }
                Err(err) => {
                    self.pause(true);
                    self.on_error(err);
                }
            }
        }
    }
}
