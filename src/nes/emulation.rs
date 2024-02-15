use crate::{
    audio::Mixer,
    common::{Reset, ResetKind},
    control_deck::ControlDeck,
    input::{JoypadBtn, JoypadBtnState, Player},
    nes::{
        config::Config,
        emulation::{replay::Replay, rewind::Rewind},
        event::{DeckEvent, Event, NesEvent},
        renderer::BufferPool,
    },
    profile,
    video::VideoFilter,
    NesError, NesResult,
};
use anyhow::{anyhow, Context};
use crossbeam::channel::{self, Sender};
use std::{
    io::{self, Read},
    path::PathBuf,
    sync::Arc,
    thread::{self, JoinHandle},
};
use web_time::Instant;
use winit::{
    event::{ElementState, Event as WinitEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoopProxy, EventLoopWindowTarget},
    window::Window,
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
pub struct State {
    config: Config,
    window: Arc<Window>,
    event_proxy: EventLoopProxy<Event>,
    control_deck: ControlDeck,
    mixer: Mixer,
    frame_pool: BufferPool,
    // A frame accumulator of partial frames for non-integer speed changes like
    // 1.5x.
    frame_accumulator: f32,
    // Keep track of last frame time so we can predict audio sync requirements for the next
    // frame.
    last_frame_time: Instant,
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
        frame_pool: BufferPool,
        window: Arc<Window>,
        event_proxy: EventLoopProxy<Event>,
        config: Config,
    ) -> Self {
        let control_deck = ControlDeck::with_config(config.clone().into());
        let sample_rate = config.audio_sample_rate / config.frame_speed;
        let mixer = Mixer::new(
            control_deck.clock_rate() / sample_rate,
            sample_rate,
            config.audio_enabled,
        );
        let rewind = Rewind::new(config.rewind_interval, config.rewind_buffer_size_mb);
        Self {
            config,
            window,
            event_proxy,
            control_deck,
            mixer,
            frame_pool,
            frame_accumulator: 0.0,
            last_frame_time: Instant::now(),
            paused: true,
            rewinding: false,
            rewind,
            replay: Replay::default(),
        }
    }

    pub fn add_message<S: ToString>(&mut self, msg: S) {
        // Can't use send_event here because it would create a cycle
        if self
            .event_proxy
            .send_event(NesEvent::Message(msg.to_string()).into())
            .is_err()
        {
            log::error!("failed to send message");
        }
    }

    pub fn send_event(&mut self, event: impl Into<Event>) {
        let event = event.into();
        log::debug!("Emulation event: {event:?}");
        if self.event_proxy.send_event(event).is_err() {
            self.on_error(anyhow!("failed to send event"));
        }
    }

    pub fn on_error(&mut self, err: NesError) {
        log::error!("Emulation error: {err:?}");
        self.add_message(err);
    }

    pub fn next_frame_time(&self) -> Instant {
        Instant::now() + self.mixer.queued_time() - self.config.audio_latency
    }

    pub fn on_event(&mut self, event: &WinitEvent<Event>) {
        match event {
            WinitEvent::WindowEvent { event, .. } => match event {
                WindowEvent::CursorMoved { position, .. } => {
                    // Aim zapper
                    if self.config.control_deck.zapper {
                        let x = (position.x / self.config.scale as f64) * 8.0 / 7.0 + 0.5; // Adjust ratio
                        let mut y = position.y / self.config.scale as f64;
                        // Account for trimming top 8 scanlines
                        if self.config.control_deck.region.is_ntsc() {
                            y += 8.0;
                        };
                        self.control_deck
                            .aim_zapper(x.round() as i32, y.round() as i32);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                WindowEvent::DroppedFile(rom) => {
                    if self.control_deck.loaded_rom().is_some() {
                        if let Err(err) = self.mixer.pause() {
                            self.on_error(err);
                        }
                    }
                    self.load_rom_path(rom);
                }
                _ => (),
            },
            WinitEvent::UserEvent(Event::ControlDeck(event)) => {
                self.on_control_deck_event(event);
            }
            _ => (),
        }
    }

    pub fn on_control_deck_event(&mut self, event: &DeckEvent) {
        if self.replay.is_playing() {
            return;
        }

        match event {
            DeckEvent::LoadRom((name, rom)) => {
                self.load_rom(name, &mut io::Cursor::new(rom));
                if let Some(ref replay) = self.config.replay_path {
                    if let Err(err) = self.replay.load(replay) {
                        self.on_error(err);
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    use winit::platform::web::WindowExtWebSys;
                    if let Err(err) = self.window.canvas().map(|canvas| canvas.focus()) {
                        log::error!("failed to focus canvas: {err:?}");
                    }
                }
            }
            DeckEvent::Joypad((player, button, state)) => {
                self.on_joypad(*player, *button, *state);
                self.replay
                    .record(self.control_deck.frame_number(), event.clone());
            }
            DeckEvent::TriggerZapper => {
                self.control_deck.trigger_zapper();
                self.replay
                    .record(self.control_deck.frame_number(), event.clone());
            }
            DeckEvent::Reset(kind) => {
                self.control_deck.reset(*kind);
                match kind {
                    ResetKind::Soft => self.add_message("Reset"),
                    ResetKind::Hard => self.add_message("Power Cycled"),
                }
            }
            DeckEvent::Pause(paused) => self.pause(*paused),
            DeckEvent::TogglePause => self.pause(!self.paused),
            DeckEvent::ToggleReplayRecord => match self.replay.toggle(self.control_deck.cpu()) {
                Ok(()) => self.add_message("Audio Recording Started"),
                Err(err) => self.on_error(err),
            },
            DeckEvent::ToggleAudioRecord => self.toggle_audio_record(),
            DeckEvent::ToggleAudio => {
                self.config.audio_enabled = !self.config.audio_enabled;
                if let Err(err) = self.mixer.set_enabled(self.config.audio_enabled) {
                    self.on_error(err);
                }
                self.send_event(NesEvent::ConfigUpdate(self.config.clone()));
                if self.config.audio_enabled {
                    self.add_message("Audio Enabled");
                } else {
                    self.add_message("Audio Disabled");
                }
            }
            DeckEvent::ToggleApuChannel(channel) => {
                self.control_deck.toggle_apu_channel(*channel);
                self.add_message(format!("Toggled APU Channel {:?}", channel));
            }
            DeckEvent::ToggleVideoFilter(filter) => {
                self.config.control_deck.filter = if self.config.control_deck.filter == *filter {
                    VideoFilter::Pixellate
                } else {
                    *filter
                };
                self.control_deck
                    .set_filter(self.config.control_deck.filter);
                self.send_event(NesEvent::ConfigUpdate(self.config.clone()));
            }
            DeckEvent::Screenshot => match self.save_screenshot() {
                Ok(filename) => {
                    self.add_message(format!("Screenshot Saved: {}", filename.display()))
                }
                Err(err) => self.on_error(err),
            },
            DeckEvent::SaveState => match self.control_deck.save_state() {
                Ok(_) => self.add_message(format!(
                    "State {} Saved",
                    self.config.control_deck.save_slot
                )),
                Err(err) => self.on_error(err),
            },
            DeckEvent::LoadState => match self.control_deck.load_state() {
                Ok(_) => self.add_message(format!(
                    "State {} Loaded",
                    self.config.control_deck.save_slot
                )),
                Err(err) => self.on_error(err),
            },
            DeckEvent::SetSaveSlot(slot) => {
                self.add_message(format!("Changed Save Slot to {}", slot));
                self.control_deck.set_save_slot(*slot);
                self.config.control_deck = self.control_deck.config().clone();
                self.send_event(NesEvent::ConfigUpdate(self.config.clone()));
            }
            DeckEvent::SetFrameSpeed(speed) => {
                self.add_message(format!("Changed Emulation Speed to {}", speed));
                self.config.frame_speed = *speed;
                let clock_rate = self.control_deck.clock_rate();
                let sample_rate = self.config.audio_sample_rate / self.config.frame_speed;
                self.send_event(NesEvent::ConfigUpdate(self.config.clone()));
                if let Err(err) = self.mixer.set_resample_ratio(clock_rate / sample_rate) {
                    self.on_error(err);
                }
            }
            DeckEvent::Rewind((state, repeat)) => self.on_rewind(*state, *repeat),
        }
    }

    pub fn pause(&mut self, paused: bool) {
        self.paused = paused;
        if self.control_deck.is_running() {
            if paused {
                if let Err(err) = self.replay.stop() {
                    self.on_error(err);
                }
                if let Err(err) = self.mixer.pause() {
                    self.on_error(err);
                }
            } else if let Err(err) = self.mixer.play() {
                self.on_error(err);
                self.add_message("Failed to resume audio");
            }
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
        self.pause(true);
    }

    fn on_load_rom(&mut self) {
        if let Some(loaded_rom) = self.control_deck.loaded_rom() {
            self.window.set_title(loaded_rom);
        }
        self.config.control_deck = self.control_deck.config().clone();
        self.send_event(NesEvent::ConfigUpdate(self.config.clone()));
        if let Err(err) = self.mixer.start() {
            self.on_error(err);
        }
        self.pause(false);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_rom_path(&mut self, path: impl AsRef<std::path::Path>) {
        self.on_unload_rom();
        match self.control_deck.load_rom_path(path) {
            Ok(()) => self.on_load_rom(),
            Err(err) => self.on_error(err),
        }
    }

    fn load_rom(&mut self, filename: &str, rom: &mut impl Read) {
        self.on_unload_rom();
        match self.control_deck.load_rom(filename, rom) {
            Ok(()) => self.on_load_rom(),
            Err(err) => self.on_error(err),
        }
    }

    pub fn toggle_audio_record(&mut self) {
        if self.control_deck.is_running() {
            if self.mixer.is_recording() {
                if let Err(err) = self.mixer.stop_recording() {
                    self.on_error(err);
                }
            } else if let Err(err) = self.mixer.start_recording() {
                self.on_error(err);
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
    }

    fn clock_frame(&mut self) {
        // Frames that aren't multiples of the default render 1 more/less frames
        // every other frame
        // e.g. a speed of 1.5 will clock # of frames: 1, 2, 1, 2, 1, 2, 1, 2, ...
        // A speed of 0.5 will clock 0, 1, 0, 1, 0, 1, 0, 1, 0, ...
        self.frame_accumulator += self.config.frame_speed;
        let mut frames_to_clock = 0;
        while self.frame_accumulator >= 1.0 {
            self.frame_accumulator -= 1.0;
            frames_to_clock += 1;
        }

        while self.mixer.queued_time() <= self.config.audio_latency && frames_to_clock > 0 {
            let now = Instant::now();
            let last_frame_duration = now - self.last_frame_time;
            self.last_frame_time = now;
            log::trace!("last frame: {:.4}s", last_frame_duration.as_secs_f32());

            if let Some(event) = self.replay.next(self.control_deck.frame_number()) {
                self.on_control_deck_event(&event);
            }

            if self
                .control_deck
                .clock_frame()
                .map_err(|err| self.on_error(err))
                .is_ok()
            {
                if self.config.rewind {
                    self.rewind.push(self.control_deck.cpu().clone());
                }
                let _ = self
                    .mixer
                    .process(self.control_deck.audio_samples())
                    .map_err(|err| self.on_error(err));
                self.control_deck.clear_audio_samples();
            }

            frames_to_clock -= 1;
        }

        if let Ok(mut frame_slot) = self.frame_pool.push_ref() {
            frame_slot.clear();
            frame_slot.extend_from_slice(self.control_deck.frame_buffer());
        }
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
    tx: Sender<WinitEvent<Event>>,
    handle: JoinHandle<()>,
}

impl Multi {
    fn spawn(
        frame_pool: BufferPool,
        window: Arc<Window>,
        event_proxy: EventLoopProxy<Event>,
        config: Config,
    ) -> NesResult<Self> {
        let (tx, rx) = channel::bounded::<WinitEvent<Event>>(1024);
        Ok(Self {
            tx,
            handle: thread::Builder::new()
                .name("emulation".into())
                .spawn(move || Self::main(frame_pool, window, event_proxy, config, rx))?,
        })
    }

    fn main(
        frame_pool: BufferPool,
        window: Arc<Window>,
        event_proxy: EventLoopProxy<Event>,
        config: Config,
        rx: channel::Receiver<WinitEvent<Event>>,
    ) {
        log::debug!("emulation thread started");
        let mut state = State::new(frame_pool, window, event_proxy, config); // Has to be created on the thread, since
        loop {
            profile!();

            while let Ok(event) = rx.try_recv() {
                state.on_event(&event);
            }

            if !state.paused {
                state.clock_frame();
            } else if state.rewinding {
                state.rewind();
            }

            std::thread::park_timeout(
                state
                    .mixer
                    .queued_time()
                    .saturating_sub(state.config.audio_latency),
            );
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
        frame_pool: BufferPool,
        window: Arc<Window>,
        event_proxy: EventLoopProxy<Event>,
        config: Config,
    ) -> NesResult<Self> {
        let threaded = config.threaded
            && thread::available_parallelism().map_or(false, |count| count.get() > 1);
        let backend = if threaded {
            Threads::Multi(Multi::spawn(frame_pool, window, event_proxy, config)?)
        } else {
            Threads::Single(Single {
                state: State::new(frame_pool, window, event_proxy, config),
            })
        };

        Ok(Self { threads: backend })
    }

    /// Handle event.
    pub fn on_event(&mut self, event: &WinitEvent<Event>) -> NesResult<()> {
        if !matches!(
            event,
            WinitEvent::AboutToWait
                | WinitEvent::NewEvents(..)
                | WinitEvent::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                }
        ) {
            match &mut self.threads {
                Threads::Single(Single { state }) => state.on_event(event),
                Threads::Multi(Multi { tx, handle }) => {
                    handle.thread().unpark();
                    log::trace!("sending emulation event: {event:?}");
                    tx.try_send(event.clone())
                        .with_context(|| anyhow!("failed to send event: {event:?}"))?;
                }
            }
        }
        Ok(())
    }

    pub fn request_clock_frame(
        &mut self,
        window_target: &EventLoopWindowTarget<Event>,
    ) -> NesResult<()> {
        if let Threads::Single(Single { ref mut state }) = self.threads {
            if !state.paused {
                state.clock_frame();
            } else if state.rewinding {
                state.rewind();
            }
            window_target.set_control_flow(ControlFlow::WaitUntil(state.next_frame_time()));
        }
        Ok(())
    }
}
