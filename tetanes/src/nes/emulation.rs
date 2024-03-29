use crate::nes::{
    audio::Audio,
    config::{Config, FrameRate},
    emulation::{replay::Record, rewind::Rewind},
    event::{EmulationEvent, NesEvent, RendererEvent, UiEvent},
    renderer::BufferPool,
};
use anyhow::anyhow;
use chrono::Local;
use crossbeam::channel::{self, Receiver, Sender};
use replay::Replay;
use std::{
    io::{self, Read},
    path::PathBuf,
    thread::JoinHandle,
};
use tetanes_core::{
    apu::Apu,
    common::{Regional, Reset, ResetKind},
    control_deck::ControlDeck,
    fs,
    ppu::Ppu,
    time::{Duration, Instant},
};
use tracing::{debug, error, trace};
use winit::{
    event::{ElementState, Event},
    event_loop::EventLoopProxy,
};

pub mod replay;
pub mod rewind;

#[derive(Debug)]
#[must_use]
enum Threads {
    Single(Single),
    Multi(Multi),
}

#[derive(Debug)]
#[must_use]
pub enum StateEvent {
    Nes(UiEvent),
    Renderer(RendererEvent),
}

impl From<UiEvent> for StateEvent {
    fn from(event: UiEvent) -> Self {
        Self::Nes(event)
    }
}

impl From<RendererEvent> for StateEvent {
    fn from(event: RendererEvent) -> Self {
        Self::Renderer(event)
    }
}

impl From<StateEvent> for NesEvent {
    fn from(event: StateEvent) -> Self {
        match event {
            StateEvent::Nes(event) => Self::Ui(event),
            StateEvent::Renderer(event) => Self::Renderer(event),
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct State {
    event_proxy: EventLoopProxy<NesEvent>,
    config: Config,
    control_deck: ControlDeck,
    audio: Audio,
    frame_pool: BufferPool,
    target_frame_duration: Duration,
    last_frame_time: Instant,
    total_frame_duration: Duration,
    frame_time_accumulator: f32,
    occluded: bool,
    paused: bool,
    rewinding: bool,
    rewind: Option<Rewind>,
    record: Option<Record>,
    replay: Option<Replay>,
}

impl Drop for State {
    fn drop(&mut self) {
        self.on_unload_rom();
    }
}

impl State {
    pub fn new(
        event_proxy: EventLoopProxy<NesEvent>,
        frame_pool: BufferPool,
        config: Config,
    ) -> Self {
        let (control_deck, audio, rewind) = config.read(|cfg| {
            let control_deck = ControlDeck::with_config(cfg.deck.clone());
            let audio = Audio::new(
                Apu::SAMPLE_RATE * cfg.emulation.speed,
                cfg.audio.latency,
                cfg.audio.buffer_size,
            );
            let rewind = cfg.emulation.rewind.then_some(Rewind::new());
            (control_deck, audio, rewind)
        });
        Self {
            event_proxy,
            config,
            control_deck,
            audio,
            frame_pool,
            target_frame_duration: Duration::default(),
            last_frame_time: Instant::now(),
            total_frame_duration: Duration::default(),
            frame_time_accumulator: 0.0,
            occluded: false,
            paused: true,
            rewinding: false,
            rewind,
            record: None,
            replay: None,
        }
    }

    pub fn add_message<S: ToString>(&mut self, msg: S) {
        self.send_event(UiEvent::Message(msg.to_string()));
    }

    pub fn send_event(&mut self, event: impl Into<StateEvent>) {
        let event = event.into();
        trace!("Emulation event: {event:?}");
        if let Err(err) = self.event_proxy.send_event(event.into()) {
            error!("failed to send emulation event: {err:?}");
            std::process::exit(1);
        }
    }

    pub fn on_error(&mut self, err: anyhow::Error) {
        error!("Emulation error: {err:?}");
        self.add_message(err);
    }

    /// Handle event.
    pub fn on_event(&mut self, event: &Event<NesEvent>) {
        if let Event::UserEvent(NesEvent::Emulation(event)) = event {
            match event {
                EmulationEvent::InstantRewind => self.instant_rewind(),
                EmulationEvent::Joypad((player, button, state)) => {
                    let pressed = *state == ElementState::Pressed;
                    let joypad = self.control_deck.joypad_mut(*player);
                    joypad.set_button(*button, pressed);
                    if let Some(ref mut replay) = self.record {
                        replay.record(self.control_deck.frame_number(), event.clone());
                    }
                }
                EmulationEvent::LoadRom((name, rom)) => {
                    self.load_rom(name, &mut io::Cursor::new(rom));
                }
                EmulationEvent::LoadRomPath(path) => self.load_rom_path(path),
                EmulationEvent::LoadReplayPath(path) => self.load_replay_path(path),
                EmulationEvent::Pause(paused) => self.pause(*paused),
                EmulationEvent::Reset(kind) => {
                    self.control_deck.reset(*kind);
                    self.pause(false);
                    match kind {
                        ResetKind::Soft => self.add_message("Reset"),
                        ResetKind::Hard => self.add_message("Power Cycled"),
                    }
                }
                EmulationEvent::Rewind(rewind) => {
                    if self.config.read(|cfg| cfg.emulation.rewind) {
                        self.rewinding = *rewind;
                    } else {
                        self.rewind_disabled();
                    }
                }
                EmulationEvent::Screenshot => match self.save_screenshot() {
                    Ok(filename) => {
                        self.add_message(format!("Screenshot Saved: {}", filename.display()));
                    }
                    Err(err) => self.on_error(err),
                },
                EmulationEvent::SetAudioEnabled(enabled) => {
                    if *enabled {
                        match self.audio.start() {
                            Ok(()) => self.add_message("Audio Enabled"),
                            Err(err) => self.on_error(err),
                        }
                    } else {
                        self.audio.stop();
                        self.add_message("Audio Disabled");
                    }
                }
                EmulationEvent::SetCycleAccurate(enabled) => {
                    self.control_deck.set_cycle_accurate(*enabled);
                }
                EmulationEvent::SetFourPlayer(four_player) => {
                    self.control_deck.set_four_player(*four_player);
                }
                EmulationEvent::SetSpeed(speed) => self.control_deck.set_frame_speed(*speed),
                EmulationEvent::SetRegion(region) => {
                    self.control_deck.set_region(*region);
                    self.target_frame_duration = FrameRate::from(*region).duration();
                }
                EmulationEvent::StateLoad => {
                    if let Some(rom) = self.control_deck.loaded_rom() {
                        let slot = self.config.read(|cfg| cfg.emulation.save_slot);
                        if let Some(path) = Config::save_path(rom, slot) {
                            match self.control_deck.load_state(path) {
                                Ok(_) => self.add_message(format!("State {slot} Loaded")),
                                Err(err) => self.on_error(err.into()),
                            }
                        }
                    }
                }
                EmulationEvent::StateSave => {
                    if let Some(rom) = self.control_deck.loaded_rom() {
                        let slot = self.config.read(|cfg| cfg.emulation.save_slot);
                        if let Some(data_dir) = Config::save_path(rom, slot) {
                            match self.control_deck.save_state(data_dir) {
                                Ok(_) => self.add_message(format!("State {slot} Saved")),
                                Err(err) => self.on_error(err.into()),
                            }
                        }
                    }
                }
                EmulationEvent::ToggleApuChannel(channel) => {
                    self.control_deck.toggle_apu_channel(*channel);
                    self.add_message(format!("Toggled APU Channel {:?}", channel));
                }
                EmulationEvent::AudioRecord(recording) => self.audio_record(*recording),
                EmulationEvent::ReplayRecord(recording) => self.replay_record(*recording),
                EmulationEvent::SetVideoFilter(filter) => self.control_deck.set_filter(*filter),
                EmulationEvent::ZapperAim((x, y)) => {
                    self.control_deck.aim_zapper(*x, *y);
                    if let Some(ref mut replay) = self.record {
                        replay.record(self.control_deck.frame_number(), event.clone());
                    }
                }
                EmulationEvent::ZapperConnect(connected) => {
                    self.control_deck.connect_zapper(*connected)
                }
                EmulationEvent::ZapperTrigger => {
                    self.control_deck.trigger_zapper();
                    if let Some(ref mut replay) = self.record {
                        replay.record(self.control_deck.frame_number(), event.clone());
                    }
                }
            }
        }
    }

    pub fn pause(&mut self, paused: bool) {
        if self.control_deck.is_running() && !self.control_deck.cpu_corrupted() {
            self.paused = paused;
            if self.paused {
                if let Some(replay) = self.record.take() {
                    if let Err(err) = replay.stop() {
                        self.on_error(err);
                    }
                }
            }
            self.audio.pause(self.paused);
        } else {
            self.paused = true;
        }
    }

    fn on_unload_rom(&mut self) {
        if let Some(rom) = self.control_deck.loaded_rom() {
            let (should_save, slot) = self
                .config
                .read(|cfg| (cfg.emulation.save_on_exit, cfg.emulation.save_slot));
            if should_save {
                if let Some(path) = Config::save_path(rom, slot) {
                    if let Err(err) = self.control_deck.save_state(path) {
                        self.on_error(err.into());
                    }
                }
            }
            self.pause(true);
            self.audio.stop();
        }
    }

    fn on_load_rom(&mut self, name: impl Into<String>) {
        let name = name.into();
        let (should_load, slot) = self
            .config
            .read(|cfg| (cfg.emulation.load_on_start, cfg.emulation.save_slot));
        if should_load {
            if let Some(path) = Config::save_path(&name, slot) {
                if let Err(err) = self.control_deck.load_state(path) {
                    error!("failed to load state: {err:?}");
                }
            }
        }
        let region = self.control_deck.region();
        self.target_frame_duration = FrameRate::from(region).duration();
        self.config.write(|cfg| cfg.deck.region = region);
        self.send_event(RendererEvent::RomLoaded(name));
        if self.config.read(|cfg| cfg.audio.enabled) {
            if let Err(err) = self.audio.start() {
                self.on_error(err);
            }
        }
        self.pause(false);
    }

    fn load_rom_path(&mut self, path: impl AsRef<std::path::Path>) {
        let path = path.as_ref();
        self.on_unload_rom();
        match self.control_deck.load_rom_path(path) {
            Ok(()) => {
                let filename = fs::filename(path);
                self.on_load_rom(filename);
            }
            Err(err) => self.on_error(err.into()),
        }
    }

    fn load_replay_path(&mut self, path: impl AsRef<std::path::Path>) {
        let path = path.as_ref();
        match Replay::load(path) {
            Ok((start, replay)) => {
                self.add_message(format!("Loaded Replay Recording {path:?}"));
                self.control_deck.load_cpu(start);
                self.replay = Some(replay);
                self.pause(false);
            }
            Err(err) => self.on_error(err),
        }
    }

    fn load_rom(&mut self, name: &str, rom: &mut impl Read) {
        self.on_unload_rom();
        match self.control_deck.load_rom(name, rom) {
            Ok(()) => self.on_load_rom(name),
            Err(err) => self.on_error(err.into()),
        }
    }

    pub fn audio_record(&mut self, recording: bool) {
        if self.control_deck.is_running() {
            if !recording && self.audio.is_recording() {
                self.audio.set_recording(false);
                self.add_message("Audio Recording Stopped");
            } else if recording {
                self.audio.set_recording(true);
                self.add_message("Audio Recording Started");
            }
        }
    }

    pub fn replay_record(&mut self, recording: bool) {
        if self.control_deck.is_running() {
            if recording {
                self.record = Some(Record::start(self.control_deck.cpu().clone()));
                self.add_message("Replay Recording Started");
            } else if let Some(replay) = self.record.take() {
                match replay.stop() {
                    Ok(filename) => {
                        self.add_message(format!("Saved Replay Recording {filename:?}"));
                    }
                    Err(err) => self.on_error(err),
                }
            }
        }
    }

    pub fn save_screenshot(&mut self) -> anyhow::Result<PathBuf> {
        let filename = PathBuf::from(
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

    fn sleep(&self) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let timeout = if self.audio.enabled() {
            self.audio.queued_time().saturating_sub(self.audio.latency)
        } else {
            (self.last_frame_time + self.target_frame_duration)
                .saturating_duration_since(Instant::now())
        };
        if timeout > Duration::from_micros(100) {
            trace!("sleeping for {:.4}s", timeout.as_secs_f32());
            std::thread::park_timeout(timeout);
        }
    }

    fn clock_frame(&mut self) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.paused || self.occluded || !self.control_deck.is_running() {
            return std::thread::park();
        }

        let last_frame_duration = self.last_frame_time.elapsed();
        trace!("last frame: {:.4}s", last_frame_duration.as_secs_f32());
        self.last_frame_time = Instant::now();
        self.total_frame_duration += last_frame_duration;
        self.frame_time_accumulator += last_frame_duration.as_secs_f32();
        if self.frame_time_accumulator > 0.25 {
            self.frame_time_accumulator = 0.25;
        }

        let mut clocked_frames = 0; // Prevent infinite loop when queued audio falls behind
        let frame_duration_seconds = self.target_frame_duration.as_secs_f32();
        while if self.audio.enabled() && !self.rewinding {
            self.audio.queued_time() < self.audio.latency && clocked_frames <= 3
        } else {
            self.frame_time_accumulator > frame_duration_seconds
        } {
            #[cfg(feature = "profiling")]
            puffin::profile_scope!("clock");

            if self.rewinding {
                match self.rewind.as_mut().and_then(|rewind| rewind.pop()) {
                    Some(cpu) => self.control_deck.load_cpu(cpu),
                    None => self.rewinding = false,
                }
            } else {
                if let Some(event) = self
                    .replay
                    .as_mut()
                    .and_then(|r| r.next(self.control_deck.frame_number()))
                {
                    self.on_event(&Event::UserEvent(event.into()));
                }

                match self.control_deck.clock_frame() {
                    Ok(_) => {
                        if let Some(ref mut rewind) = self.rewind {
                            rewind.push(self.control_deck.cpu());
                        }
                        self.audio.process(self.control_deck.audio_samples());
                        self.control_deck.clear_audio_samples();
                    }
                    Err(err) => {
                        self.on_error(err.into());
                        self.pause(true);
                    }
                }
            }
            self.frame_time_accumulator -= frame_duration_seconds;
            clocked_frames += 1;
        }

        let mut new_frame = false;
        if let Ok(mut frame) = self.frame_pool.push_ref() {
            frame.clear();
            frame.extend_from_slice(self.control_deck.frame_buffer());
            new_frame = true;
        }
        if new_frame {
            self.send_event(RendererEvent::Frame(self.total_frame_duration));
            self.total_frame_duration = Duration::default();
        }

        self.sleep();
    }
}

#[derive(Debug)]
#[must_use]
struct Single {
    state: State,
}

#[derive(Debug)]
#[must_use]
struct Multi {
    tx: Sender<Event<NesEvent>>,
    handle: JoinHandle<()>,
}

impl Multi {
    fn spawn(
        event_proxy: EventLoopProxy<NesEvent>,
        frame_pool: BufferPool,
        config: Config,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = channel::bounded(1024);
        Ok(Self {
            tx,
            handle: std::thread::Builder::new()
                .name("emulation".into())
                .spawn(move || Self::main(event_proxy, rx, frame_pool, config))?,
        })
    }

    fn main(
        event_proxy: EventLoopProxy<NesEvent>,
        rx: Receiver<Event<NesEvent>>,
        frame_pool: BufferPool,
        config: Config,
    ) {
        debug!("emulation thread started");
        let mut state = State::new(event_proxy, frame_pool, config); // Has to be created on the thread, since
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
    pub fn initialize(
        event_proxy: EventLoopProxy<NesEvent>,
        frame_pool: BufferPool,
        config: Config,
    ) -> anyhow::Result<Self> {
        let threaded = config.read(|cfg| cfg.emulation.threaded)
            && std::thread::available_parallelism().map_or(false, |count| count.get() > 1);
        let backend = if threaded {
            Threads::Multi(Multi::spawn(event_proxy, frame_pool, config)?)
        } else {
            Threads::Single(Single {
                state: State::new(event_proxy, frame_pool, config),
            })
        };

        Ok(Self { threads: backend })
    }

    /// Handle event.
    pub fn on_event(&mut self, event: &Event<NesEvent>) {
        match &mut self.threads {
            Threads::Single(Single { state }) => state.on_event(event),
            Threads::Multi(Multi { tx, handle }) => {
                handle.thread().unpark();
                if let Err(err) = tx.try_send(event.clone()) {
                    error!("failed to send event to emulation thread: {event:?}. {err:?}");
                    std::process::exit(1);
                }
            }
        }
    }

    pub fn request_clock_frame(&mut self) -> anyhow::Result<()> {
        // Multi-threaded emulation will handle frame clocking on its own
        if let Threads::Single(Single { ref mut state }) = self.threads {
            state.clock_frame();
        }
        Ok(())
    }
}
