use crate::nes::{
    audio::Audio,
    config::Config,
    emulation::{replay::Replay, rewind::Rewind},
    event::{EmulationEvent, NesEvent, RendererEvent, UiEvent},
    renderer::BufferPool,
};
use anyhow::anyhow;
use crossbeam::channel::{self, Receiver, Sender};
use std::{
    io::{self, Read},
    path::PathBuf,
    thread::JoinHandle,
};
use tetanes_core::{
    apu::Apu,
    common::{Regional, Reset, ResetKind},
    control_deck::ControlDeck,
};
use tetanes_util::{
    platform::time::{Duration, Instant},
    NesError, NesResult,
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
    control_deck: ControlDeck,
    audio: Audio,
    frame_pool: BufferPool,
    target_frame_duration: Duration,
    last_frame_time: Instant,
    frame_time_accumulator: f32,
    last_frame_event: Instant,
    occluded: bool,
    paused: bool,
    rewinding: bool,
    rewind: Rewind,
    replay: Replay,
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
        let control_deck = ControlDeck::with_config(config.clone().into());
        let sample_rate = config.audio_sample_rate;
        let audio = Audio::new(
            Apu::SAMPLE_RATE * f32::from(config.frame_speed),
            sample_rate,
            config.audio_latency,
            config.audio_buffer_size,
        );
        let rewind = Rewind::new(
            config.rewind,
            config.rewind_interval,
            config.rewind_buffer_size_mb,
        );
        let frame_time_accumulator = config.target_frame_duration.as_secs_f32();
        Self {
            event_proxy,
            control_deck,
            audio,
            frame_pool,
            target_frame_duration: config.target_frame_duration,
            last_frame_time: Instant::now(),
            frame_time_accumulator,
            last_frame_event: Instant::now(),
            occluded: false,
            paused: true,
            rewinding: false,
            rewind,
            replay: Replay::default(),
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

    pub fn on_error(&mut self, err: NesError) {
        error!("Emulation error: {err:?}");
        self.add_message(err);
    }

    /// Handle event.
    pub fn on_event(&mut self, event: &Event<NesEvent>) {
        if self.replay.is_playing() {
            return;
        }

        if let Event::UserEvent(NesEvent::Emulation(event)) = event {
            match event {
                EmulationEvent::Joypad((player, button, state)) => {
                    let pressed = *state == ElementState::Pressed;
                    let joypad = self.control_deck.joypad_mut(*player);
                    joypad.set_button(*button, pressed);
                    self.replay
                        .record(self.control_deck.frame_number(), event.clone());
                }
                #[cfg(not(target_arch = "wasm32"))]
                EmulationEvent::LoadRomPath((path, config)) => self.load_rom_path(path, config),
                EmulationEvent::LoadRom((name, rom, config)) => {
                    self.load_rom(name, &mut io::Cursor::new(rom), config)
                }
                EmulationEvent::TogglePause => self.pause(!self.paused),
                EmulationEvent::Pause(paused) => self.pause(*paused),
                EmulationEvent::Reset(kind) => {
                    self.control_deck.reset(*kind);
                    self.pause(false);
                    match kind {
                        ResetKind::Soft => self.add_message("Reset"),
                        ResetKind::Hard => self.add_message("Power Cycled"),
                    }
                }
                EmulationEvent::Rewind((state, repeat)) => self.on_rewind(*state, *repeat),
                EmulationEvent::Screenshot => match self.save_screenshot() {
                    Ok(filename) => {
                        self.add_message(format!("Screenshot Saved: {}", filename.display()))
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
                EmulationEvent::SetFrameSpeed(speed) => self
                    .audio
                    .set_input_rate(self.control_deck.clock_rate() * f32::from(speed)),
                EmulationEvent::SetRegion(region) => self.control_deck.set_region(*region),
                EmulationEvent::SetTargetFrameDuration(duration) => {
                    self.target_frame_duration = *duration
                }
                EmulationEvent::StateLoad(config) => {
                    match self.control_deck.load_state(Some(config)) {
                        Ok(_) => self.add_message(format!("State {} Loaded", config.save_slot)),
                        Err(err) => self.on_error(err),
                    }
                }
                EmulationEvent::StateSave(config) => {
                    match self.control_deck.save_state(Some(config)) {
                        Ok(_) => self.add_message(format!("State {} Saved", config.save_slot)),
                        Err(err) => self.on_error(err),
                    }
                }
                EmulationEvent::ToggleApuChannel(channel) => {
                    self.control_deck.toggle_apu_channel(*channel);
                    self.add_message(format!("Toggled APU Channel {:?}", channel));
                }
                EmulationEvent::ToggleAudioRecord => self.toggle_audio_record(),
                EmulationEvent::ToggleReplayRecord => {
                    match self.replay.toggle(self.control_deck.cpu()) {
                        Ok(()) => self.add_message("Audio Recording Started"),
                        Err(err) => self.on_error(err),
                    }
                }
                EmulationEvent::SetVideoFilter(filter) => self.control_deck.set_filter(*filter),
                EmulationEvent::ZapperAim((x, y)) => {
                    self.control_deck.aim_zapper(*x, *y);
                    self.replay
                        .record(self.control_deck.frame_number(), event.clone());
                }
                EmulationEvent::ZapperConnect(connected) => {
                    self.control_deck.connect_zapper(*connected)
                }
                EmulationEvent::ZapperTrigger => {
                    self.control_deck.trigger_zapper();
                    self.replay
                        .record(self.control_deck.frame_number(), event.clone());
                }
            }
        }
    }

    pub fn pause(&mut self, paused: bool) {
        if self.control_deck.is_running() && !self.control_deck.cpu_corrupted() {
            self.paused = paused;
            if self.paused {
                if let Err(err) = self.replay.stop() {
                    self.on_error(err);
                }
            }
            if let Err(err) = self.audio.pause(self.paused) {
                self.on_error(err);
            }
        } else {
            self.paused = true;
        }
    }

    fn on_unload_rom(&mut self) {
        if self.control_deck.loaded_rom().is_some() {
            self.pause(true);
            self.audio.stop();
        }
    }

    fn on_load_rom(&mut self, name: String, config: &Config) {
        self.send_event(UiEvent::SetTitle(name));
        if config.audio_enabled {
            if let Err(err) = self.audio.start() {
                self.on_error(err);
            }
        }
        if let Some(ref replay) = config.replay_path {
            if let Err(err) = self.replay.load(replay) {
                self.on_error(err);
            }
        }
        self.pause(false);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_rom_path(&mut self, path: impl AsRef<std::path::Path>, config: &Config) {
        self.on_unload_rom();
        match self.control_deck.load_rom_path(path, Some(&config.deck)) {
            Ok(name) => self.on_load_rom(name, config),
            Err(err) => self.on_error(err),
        }
    }

    fn load_rom(&mut self, name: &str, rom: &mut impl Read, config: &Config) {
        self.on_unload_rom();
        match self.control_deck.load_rom(name, rom, Some(&config.deck)) {
            Ok(()) => self.on_load_rom(name.to_string(), config),
            Err(err) => self.on_error(err),
        }
    }

    pub fn toggle_audio_record(&mut self) {
        if self.control_deck.is_running() {
            if self.audio.is_recording() {
                self.audio.set_recording(false);
            } else {
                self.audio.set_recording(true);
            }
        }
    }

    pub fn save_screenshot(&mut self) -> NesResult<PathBuf> {
        // TODO: Provide download file for WASM
        #[cfg(not(target_arch = "wasm32"))]
        {
            use chrono::Local;
            use tetanes_core::ppu::Ppu;

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

            Ok(image.save(&filename).map(|_| filename)?)
        }

        #[cfg(target_arch = "wasm32")]
        Err(anyhow!("screenshot not implemented for web yet"))
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
        if timeout > Duration::from_millis(1) {
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

        let last_frame_duration = self
            .last_frame_time
            .elapsed()
            .min(Duration::from_millis(25));
        self.last_frame_time = Instant::now();
        self.frame_time_accumulator += last_frame_duration.as_secs_f32();

        if self.rewinding {
            self.rewind();
        }

        let mut clocked_frames = 0; // Prevent infinite loop when queued audio falls behind
        let frame_duration_seconds = self.target_frame_duration.as_secs_f32();
        while if self.audio.enabled() {
            self.audio.queued_time() <= self.audio.latency && clocked_frames <= 3
        } else {
            self.frame_time_accumulator >= frame_duration_seconds
        } {
            #[cfg(feature = "profiling")]
            puffin::profile_scope!("clock");

            if let Some(event) = self.replay.next(self.control_deck.frame_number()) {
                self.on_event(&Event::UserEvent(event.into()));
            }

            match self.control_deck.clock_frame() {
                Ok(_) => {
                    self.rewind.push(self.control_deck.cpu());
                    self.audio.process(self.control_deck.audio_samples());
                    self.control_deck.clear_audio_samples();
                }
                Err(err) => {
                    self.on_error(err);
                    self.pause(true);
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
        if new_frame && self.last_frame_event.elapsed() > Duration::from_millis(200) {
            self.send_event(RendererEvent::Frame(last_frame_duration));
            self.last_frame_event = Instant::now();
            trace!("last frame: {:.4}s", last_frame_duration.as_secs_f32());
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
    ) -> NesResult<Self> {
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
    ) -> NesResult<Self> {
        let threaded = config.threaded
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

    pub fn request_clock_frame(&mut self) -> NesResult<()> {
        // Multi-threaded emulation will handle frame clocking on its own
        if let Threads::Single(Single { ref mut state }) = self.threads {
            state.clock_frame();
        }
        Ok(())
    }
}
