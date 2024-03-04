use crate::{
    audio::Mixer,
    common::{Regional, Reset, ResetKind},
    control_deck::ControlDeck,
    input::{JoypadBtn, JoypadBtnState, Player},
    nes::{
        config::Config,
        emulation::{replay::Replay, rewind::Rewind},
        event::{EmulationEvent, Event, NesEvent, RendererEvent},
        renderer::BufferPool,
    },
    platform::time::{Duration, Instant},
    profile, NesError, NesResult,
};
use anyhow::anyhow;
use crossbeam::channel::{self, Receiver, Sender};
use std::{
    io::{self, Read},
    path::PathBuf,
    thread::JoinHandle,
};
use tracing::{debug, error, trace};
use winit::event::ElementState;

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
    Nes(NesEvent),
    Renderer(RendererEvent),
}

impl From<NesEvent> for StateEvent {
    fn from(event: NesEvent) -> Self {
        Self::Nes(event)
    }
}

impl From<RendererEvent> for StateEvent {
    fn from(event: RendererEvent) -> Self {
        Self::Renderer(event)
    }
}

impl From<StateEvent> for Event {
    fn from(event: StateEvent) -> Self {
        match event {
            StateEvent::Nes(event) => Self::Nes(event),
            StateEvent::Renderer(event) => Self::Renderer(event),
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct State {
    config: Config,
    event_tx: Sender<Event>,
    control_deck: ControlDeck,
    mixer: Mixer,
    frame_pool: BufferPool,
    last_frame_time: Instant,
    frame_time_accumulator: f32,
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
    pub fn new(event_tx: Sender<Event>, frame_pool: BufferPool, config: Config) -> Self {
        let control_deck = ControlDeck::with_config(config.clone().into());
        let sample_rate = config.audio_sample_rate;
        let mixer = Mixer::new(
            control_deck.clock_rate() * f32::from(config.frame_speed),
            sample_rate,
            config.audio_latency,
        );
        let rewind = Rewind::new(
            config.rewind,
            config.rewind_interval,
            config.rewind_buffer_size_mb,
        );
        let frame_time_accumulator = config.target_frame_duration.as_secs_f32();
        Self {
            config,
            event_tx,
            control_deck,
            mixer,
            frame_pool,
            frame_time_accumulator,
            last_frame_time: Instant::now(),
            occluded: false,
            paused: true,
            rewinding: false,
            rewind,
            replay: Replay::default(),
        }
    }

    pub fn add_message<S: ToString>(&mut self, msg: S) {
        self.send_event(NesEvent::Message(msg.to_string()));
    }

    pub fn send_event(&mut self, event: impl Into<StateEvent>) {
        let event = event.into();
        trace!("Emulation event: {event:?}");
        if let Err(err) = self.event_tx.send(event.into()) {
            error!("failed to send emulation event: {err:?}");
            std::process::exit(1);
        }
    }

    pub fn on_error(&mut self, err: NesError) {
        error!("Emulation error: {err:?}");
        self.add_message(err);
    }

    pub fn on_event(&mut self, event: EmulationEvent) {
        if self.replay.is_playing() {
            return;
        }

        match event {
            EmulationEvent::Joypad((player, button, state)) => {
                self.on_joypad(player, button, state);
                self.replay
                    .record(self.control_deck.frame_number(), event.clone());
            }
            #[cfg(not(target_arch = "wasm32"))]
            EmulationEvent::LoadRomPath(path) => self.load_rom_path(path),
            EmulationEvent::LoadRom((name, rom)) => self.load_rom(&name, &mut io::Cursor::new(rom)),
            EmulationEvent::Pause(paused) => self.pause(paused),
            EmulationEvent::Reset(kind) => {
                self.control_deck.reset(kind);
                self.pause(false);
                match kind {
                    ResetKind::Soft => self.add_message("Reset"),
                    ResetKind::Hard => self.add_message("Power Cycled"),
                }
            }
            EmulationEvent::Rewind((state, repeat)) => self.on_rewind(state, repeat),
            EmulationEvent::Screenshot => match self.save_screenshot() {
                Ok(filename) => {
                    self.add_message(format!("Screenshot Saved: {}", filename.display()))
                }
                Err(err) => self.on_error(err),
            },
            EmulationEvent::SetAudioEnabled(enabled) => {
                self.config.audio_enabled = enabled;
                if self.config.audio_enabled {
                    match self.mixer.start() {
                        Ok(sample_rate) => {
                            self.config.audio_sample_rate = sample_rate;
                            self.add_message("Audio Enabled");
                        }
                        Err(err) => self.on_error(err),
                    }
                } else {
                    match self.mixer.stop() {
                        Ok(()) => self.add_message("Audio Disabled"),
                        Err(err) => self.on_error(err),
                    }
                }
            }
            EmulationEvent::SetFrameSpeed(speed) => {
                self.config.set_frame_speed(speed);
                self.mixer
                    .set_input_rate(self.control_deck.clock_rate() * f32::from(speed));
            }
            EmulationEvent::SetHideOverscan(hidden) => self.config.hide_overscan = hidden,
            EmulationEvent::SetRegion(region) => {
                self.config.set_region(region);
                self.control_deck.set_region(region);
            }
            EmulationEvent::SetSaveSlot(slot) => self.config.deck.save_slot = slot,
            EmulationEvent::StateLoad => {
                match self.control_deck.load_state(Some(&self.config.deck)) {
                    Ok(_) => {
                        self.add_message(format!("State {} Loaded", self.config.deck.save_slot))
                    }
                    Err(err) => self.on_error(err),
                }
            }
            EmulationEvent::StateSave => {
                match self.control_deck.save_state(Some(&self.config.deck)) {
                    Ok(_) => {
                        self.add_message(format!("State {} Saved", self.config.deck.save_slot))
                    }
                    Err(err) => self.on_error(err),
                }
            }
            EmulationEvent::ToggleApuChannel(channel) => {
                self.control_deck.toggle_apu_channel(channel);
                self.add_message(format!("Toggled APU Channel {:?}", channel));
            }
            EmulationEvent::ToggleAudioRecord => self.toggle_audio_record(),
            EmulationEvent::ToggleReplayRecord => match self.replay.toggle(self.control_deck.cpu())
            {
                Ok(()) => self.add_message("Audio Recording Started"),
                Err(err) => self.on_error(err),
            },
            EmulationEvent::SetVideoFilter(filter) => {
                self.config.deck.filter = filter;
                self.control_deck.set_filter(filter);
            }
            EmulationEvent::ZapperAim((x, y)) => self.control_deck.aim_zapper(x, y),
            EmulationEvent::ZapperConnect(connected) => {
                self.config.deck.zapper = connected;
                self.control_deck.connect_zapper(connected);
            }
            EmulationEvent::ZapperTrigger => {
                self.control_deck.trigger_zapper();
                self.replay
                    .record(self.control_deck.frame_number(), event.clone());
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
            if let Err(err) = self.mixer.pause(self.paused) {
                self.on_error(err);
            }
        } else {
            self.paused = true;
        }
    }

    fn on_joypad(&mut self, player: Player, button: JoypadBtn, state: ElementState) {
        let pressed = state == ElementState::Pressed;
        let joypad = self.control_deck.joypad_mut(player);
        if !self.config.concurrent_dpad && pressed {
            match button {
                JoypadBtn::Left => joypad.set_button(JoypadBtnState::RIGHT, false),
                JoypadBtn::Right => joypad.set_button(JoypadBtnState::LEFT, false),
                JoypadBtn::Up => joypad.set_button(JoypadBtnState::DOWN, false),
                JoypadBtn::Down => joypad.set_button(JoypadBtnState::UP, false),
                _ => (),
            }
        }
        joypad.set_button(button.into(), pressed);
    }

    fn on_unload_rom(&mut self) {
        if self.control_deck.loaded_rom().is_some() {
            self.pause(true);
            if let Err(err) = self.mixer.stop() {
                self.on_error(err);
            }
        }
    }

    fn on_load_rom(&mut self, name: String) {
        self.send_event(NesEvent::SetTitle(name));
        if self.config.audio_enabled {
            match self.mixer.start() {
                Ok(sample_rate) => self.config.audio_sample_rate = sample_rate,
                Err(err) => {
                    self.on_error(err);
                }
            }
        }
        if let Some(ref replay) = self.config.replay_path {
            if let Err(err) = self.replay.load(replay) {
                self.on_error(err);
            }
        }
        self.pause(false);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_rom_path(&mut self, path: impl AsRef<std::path::Path>) {
        self.on_unload_rom();
        match self
            .control_deck
            .load_rom_path(path, Some(&self.config.deck))
        {
            Ok(name) => self.on_load_rom(name),
            Err(err) => self.on_error(err),
        }
    }

    fn load_rom(&mut self, name: &str, rom: &mut impl Read) {
        self.on_unload_rom();
        match self
            .control_deck
            .load_rom(name, rom, Some(&self.config.deck))
        {
            Ok(()) => self.on_load_rom(name.to_string()),
            Err(err) => self.on_error(err),
        }
    }

    pub fn toggle_audio_record(&mut self) {
        if self.control_deck.is_running() {
            if self.mixer.is_recording() {
                self.mixer.set_recording(false);
            } else {
                self.mixer.set_recording(true);
            }
        }
    }

    pub fn save_screenshot(&mut self) -> NesResult<PathBuf> {
        // TODO: Provide download file for WASM
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::ppu::Ppu;
            use chrono::Local;

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

    pub fn remaining_frame_time(&self) -> Duration {
        if self.config.audio_enabled {
            self.mixer
                .queued_time()
                .saturating_sub(self.config.audio_latency)
        } else {
            let next_frame_time = self.last_frame_time + self.config.target_frame_duration;
            let now = Instant::now();
            if next_frame_time > now {
                next_frame_time - now
            } else {
                Duration::from_secs(0)
            }
        }
    }

    fn should_clock_frame(&mut self, frame_duration_seconds: f32) -> bool {
        if self.config.audio_enabled {
            self.mixer.queued_time() <= self.config.audio_latency
        } else {
            self.frame_time_accumulator >= frame_duration_seconds
        }
    }

    fn sleep(&self) {
        profile!("sleep");
        #[cfg(not(target_arch = "wasm32"))]
        loop {
            let timeout = self.remaining_frame_time();
            if timeout <= Duration::from_millis(1) {
                break;
            }
            trace!("sleeping for {:.4}s", timeout.as_secs_f32());
            std::thread::park_timeout(timeout);
        }
    }

    fn clock_frame(&mut self) {
        profile!();

        if self.paused || self.occluded || !self.control_deck.is_running() {
            return self.sleep();
        }

        let last_frame_duration = self
            .last_frame_time
            .elapsed()
            .min(Duration::from_millis(25));
        self.last_frame_time = Instant::now();
        self.frame_time_accumulator += last_frame_duration.as_secs_f32();
        trace!("last frame: {:.4}s", last_frame_duration.as_secs_f32());

        // TODO: fix rewind
        // if self.rewinding {
        //     self.rewind();
        // }

        let frame_duration_seconds = self.config.target_frame_duration.as_secs_f32();
        while self.should_clock_frame(frame_duration_seconds) {
            trace!(
                "queued_audio_time: {:.4}s",
                self.mixer.queued_time().as_secs_f32()
            );

            let start = Instant::now();
            if let Some(event) = self.replay.next(self.control_deck.frame_number()) {
                self.on_event(event);
            }

            match self.control_deck.clock_frame() {
                Ok(_) => {
                    self.rewind.push(self.control_deck.cpu());
                    let _ = self
                        .mixer
                        .process(self.control_deck.audio_samples())
                        .map_err(|err| self.on_error(err));
                    self.control_deck.clear_audio_samples();
                }
                Err(err) => {
                    self.on_error(err);
                    self.pause(true);
                }
            }
            self.frame_time_accumulator -= frame_duration_seconds;
            trace!("clock: {:.4}s", start.elapsed().as_secs_f32());
        }

        if let Ok(mut frame) = self.frame_pool.push_ref() {
            let start = Instant::now();
            frame.clear();
            frame.extend_from_slice(self.control_deck.frame_buffer());
            trace!("copy: {:.4}s", start.elapsed().as_secs_f32());
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
    tx: Sender<EmulationEvent>,
    handle: JoinHandle<()>,
}

impl Multi {
    fn spawn(event_tx: Sender<Event>, frame_pool: BufferPool, config: Config) -> NesResult<Self> {
        let (tx, rx) = channel::bounded(1024);
        Ok(Self {
            tx,
            handle: std::thread::Builder::new()
                .name("emulation".into())
                .spawn(move || Self::main(event_tx, rx, frame_pool, config))?,
        })
    }

    fn main(
        tx: Sender<Event>,
        rx: Receiver<EmulationEvent>,
        frame_pool: BufferPool,
        config: Config,
    ) {
        debug!("emulation thread started");
        let mut state = State::new(tx, frame_pool, config); // Has to be created on the thread, since
        loop {
            profile!();

            while let Ok(event) = rx.try_recv() {
                state.on_event(event);
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
        event_tx: Sender<Event>,
        frame_pool: BufferPool,
        config: Config,
    ) -> NesResult<Self> {
        let threaded = config.threaded
            && std::thread::available_parallelism().map_or(false, |count| count.get() > 1);
        let backend = if threaded {
            Threads::Multi(Multi::spawn(event_tx, frame_pool, config)?)
        } else {
            Threads::Single(Single {
                state: State::new(event_tx, frame_pool, config),
            })
        };

        Ok(Self { threads: backend })
    }

    /// Handle event.
    pub fn on_event(&mut self, event: EmulationEvent) {
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
