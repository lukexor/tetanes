use crate::{
    audio::Mixer,
    common::{Regional, Reset, ResetKind},
    control_deck::ControlDeck,
    input::{JoypadBtn, JoypadBtnState, Player},
    nes::{
        config::Config,
        emulation::{replay::Replay, rewind::Rewind},
        event::{DeckEvent, Event, NesEvent, RendererEvent},
        renderer::BufferPool,
    },
    platform::{
        thread,
        time::{Duration, Instant},
    },
    profile, NesError, NesResult,
};
use anyhow::anyhow;
use crossbeam::channel::{self, Sender};
use std::{
    io::{self, Read},
    path::PathBuf,
    thread::JoinHandle,
};
use winit::{
    event::{ElementState, Event as WinitEvent, WindowEvent},
    event_loop::{EventLoop, EventLoopProxy},
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
    pub fn new(frame_pool: BufferPool, event_proxy: EventLoopProxy<Event>, config: Config) -> Self {
        let control_deck = ControlDeck::with_config(config.clone().into());
        let sample_rate = config.audio_sample_rate;
        let mixer = Mixer::new(
            control_deck.clock_rate() / (sample_rate / f32::from(config.frame_speed)),
            sample_rate,
            config.audio_enabled,
        );
        let rewind = Rewind::new(config.rewind_interval, config.rewind_buffer_size_mb);
        Self {
            config,
            event_proxy,
            control_deck,
            mixer,
            frame_pool,
            frame_accumulator: 0.0,
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

    pub fn send_event(&mut self, event: impl Into<Event>) {
        let event = event.into();
        log::trace!("Emulation event: {event:?}");
        if let Err(err) = self.event_proxy.send_event(event) {
            log::error!("failed to send emulation event: {err:?}");
            std::process::exit(1);
        }
    }

    pub fn on_error(&mut self, err: NesError) {
        log::error!("Emulation error: {err:?}");
        self.add_message(err);
    }

    pub fn on_event(&mut self, event: &WinitEvent<Event>) {
        profile!();

        match event {
            #[cfg(not(target_arch = "wasm32"))]
            WinitEvent::WindowEvent {
                event: WindowEvent::DroppedFile(rom),
                ..
            } => {
                if self.control_deck.loaded_rom().is_some() {
                    if let Err(err) = self.mixer.pause() {
                        self.on_error(err);
                    }
                }
                self.load_rom_path(rom);
            }
            WinitEvent::UserEvent(Event::ControlDeck(event)) => self.on_control_deck_event(event),
            _ => (),
        }
    }

    pub fn on_control_deck_event(&mut self, event: &DeckEvent) {
        if self.replay.is_playing() {
            return;
        }

        match event {
            DeckEvent::Joypad((player, button, state)) => {
                self.on_joypad(*player, *button, *state);
                self.replay
                    .record(self.control_deck.frame_number(), event.clone());
            }
            DeckEvent::LoadRom((name, rom)) => {
                self.load_rom(name, &mut io::Cursor::new(rom));
                if let Some(ref replay) = self.config.replay_path {
                    if let Err(err) = self.replay.load(replay) {
                        self.on_error(err);
                    }
                }
            }
            DeckEvent::Occluded(occluded) => self.occluded = *occluded,
            DeckEvent::Pause(paused) => self.pause(*paused),
            DeckEvent::Reset(kind) => {
                self.control_deck.reset(*kind);
                self.pause(false);
                match kind {
                    ResetKind::Soft => self.add_message("Reset"),
                    ResetKind::Hard => self.add_message("Power Cycled"),
                }
            }
            DeckEvent::Rewind((state, repeat)) => self.on_rewind(*state, *repeat),
            DeckEvent::Screenshot => match self.save_screenshot() {
                Ok(filename) => {
                    self.add_message(format!("Screenshot Saved: {}", filename.display()))
                }
                Err(err) => self.on_error(err),
            },
            DeckEvent::SetAudioEnabled(enabled) => {
                self.config.audio_enabled = *enabled;
                match self.mixer.set_enabled(self.config.audio_enabled) {
                    Ok(()) => {
                        if self.config.audio_enabled {
                            self.add_message("Audio Enabled");
                        } else {
                            self.add_message("Audio Disabled");
                        }
                    }
                    Err(err) => self.on_error(err),
                }
            }
            DeckEvent::SetFrameSpeed(speed) => {
                self.config.frame_speed = *speed;
                let clock_rate = self.control_deck.clock_rate();
                let sample_rate = self.config.audio_sample_rate / f32::from(*speed);
                if let Err(err) = self.mixer.set_resample_ratio(clock_rate / sample_rate) {
                    self.on_error(err);
                }
            }
            DeckEvent::SetHideOverscan(hidden) => self.config.hide_overscan = *hidden,
            DeckEvent::SetRegion(region) => {
                self.config.set_region(*region);
                self.control_deck.set_region(*region);
            }
            DeckEvent::SetSaveSlot(slot) => self.config.deck.save_slot = *slot,
            DeckEvent::StateLoad => match self.control_deck.load_state(Some(&self.config.deck)) {
                Ok(_) => self.add_message(format!("State {} Loaded", self.config.deck.save_slot)),
                Err(err) => self.on_error(err),
            },
            DeckEvent::StateSave => match self.control_deck.save_state(Some(&self.config.deck)) {
                Ok(_) => self.add_message(format!("State {} Saved", self.config.deck.save_slot)),
                Err(err) => self.on_error(err),
            },
            DeckEvent::ToggleApuChannel(channel) => {
                self.control_deck.toggle_apu_channel(*channel);
                self.add_message(format!("Toggled APU Channel {:?}", channel));
            }
            DeckEvent::ToggleAudioRecord => self.toggle_audio_record(),
            DeckEvent::ToggleReplayRecord => match self.replay.toggle(self.control_deck.cpu()) {
                Ok(()) => self.add_message("Audio Recording Started"),
                Err(err) => self.on_error(err),
            },
            DeckEvent::SetVideoFilter(filter) => {
                self.config.deck.filter = *filter;
                self.control_deck.set_filter(*filter);
            }
            DeckEvent::ZapperAim((x, y)) => self.control_deck.aim_zapper(*x, *y),
            DeckEvent::ZapperConnect(connected) => {
                self.config.deck.zapper = *connected;
                self.control_deck.connect_zapper(*connected);
            }
            DeckEvent::ZapperTrigger => {
                self.control_deck.trigger_zapper();
                self.replay
                    .record(self.control_deck.frame_number(), event.clone());
            }
        }
    }

    pub fn pause(&mut self, paused: bool) {
        if self.control_deck.is_running() && !self.control_deck.cpu_corrupted() {
            self.paused = paused;
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
        self.pause(true);
    }

    fn on_load_rom(&mut self) {
        // TODO: Move to main thread
        // if let Some(loaded_rom) = self.control_deck.loaded_rom() {
        //     self.window.set_title(loaded_rom);
        // }
        if let Err(err) = self.mixer.start() {
            self.on_error(err);
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
            Ok(()) => self.on_load_rom(),
            Err(err) => self.on_error(err),
        }
    }

    fn load_rom(&mut self, filename: &str, rom: &mut impl Read) {
        self.on_unload_rom();
        match self
            .control_deck
            .load_rom(filename, rom, Some(&self.config.deck))
        {
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

        #[cfg(target_arch = "wasm32")]
        Err(anyhow!("screenshot not implemented for web yet"))
    }

    pub fn remaining_frame_time(&self) -> Duration {
        self.mixer
            .queued_time()
            .saturating_sub(self.config.audio_latency)
    }

    fn clock_frame(&mut self) {
        profile!();

        if self.paused || self.occluded {
            profile!("sleep");
            return thread::sleep(self.config.target_frame_duration);
        }
        if self.rewinding {
            return self.rewind();
        }

        // Frames that aren't multiples of the default render 1 more/less frames
        // every other frame
        // e.g. a speed of 1.5 will clock # of frames: 1, 2, 1, 2, 1, 2, 1, 2, ...
        // A speed of 0.5 will clock 0, 1, 0, 1, 0, 1, 0, 1, 0, ...
        self.frame_accumulator += f32::from(self.config.frame_speed);
        let mut frames_to_clock = 0;
        while self.frame_accumulator >= 1.0 {
            self.frame_accumulator -= 1.0;
            frames_to_clock += 1;
        }

        let now = Instant::now();
        let last_frame_duration = now - self.last_frame_time;
        self.last_frame_time = now;
        log::trace!("last frame: {:.4}s", last_frame_duration.as_secs_f32());
        self.send_event(RendererEvent::Frame(last_frame_duration));

        while self.mixer.queued_time() <= self.config.audio_latency && frames_to_clock > 0 {
            if let Some(event) = self.replay.next(self.control_deck.frame_number()) {
                self.on_control_deck_event(&event);
            }

            match self.control_deck.clock_frame() {
                Ok(_) => {
                    if self.config.rewind {
                        self.rewind.push(self.control_deck.cpu().clone());
                    }
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

            frames_to_clock -= 1;
        }

        if let Ok(mut frame) = self.frame_pool.push_ref() {
            frame.clear();
            frame.extend_from_slice(self.control_deck.frame_buffer());
        }

        crate::profiling::end_frame();
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
        event_proxy: EventLoopProxy<Event>,
        config: Config,
    ) -> NesResult<Self> {
        let (tx, rx) = channel::bounded::<WinitEvent<Event>>(1024);
        Ok(Self {
            tx,
            handle: std::thread::Builder::new()
                .name("emulation".into())
                .spawn(move || Self::main(frame_pool, event_proxy, config, rx))?,
        })
    }

    fn main(
        frame_pool: BufferPool,
        event_proxy: EventLoopProxy<Event>,
        config: Config,
        rx: channel::Receiver<WinitEvent<Event>>,
    ) {
        log::debug!("emulation thread started");
        let mut state = State::new(frame_pool, event_proxy, config); // Has to be created on the thread, since
        loop {
            profile!();

            while let Ok(event) = rx.try_recv() {
                state.on_event(&event);
            }

            state.clock_frame();

            {
                profile!("park");
                std::thread::park_timeout(state.remaining_frame_time());
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
    pub fn initialize(
        event: &EventLoop<Event>,
        frame_pool: BufferPool,
        config: Config,
    ) -> NesResult<Self> {
        let event_proxy = event.create_proxy();
        let threaded = config.threaded
            && std::thread::available_parallelism().map_or(false, |count| count.get() > 1);
        let backend = if threaded {
            Threads::Multi(Multi::spawn(frame_pool, event_proxy, config)?)
        } else {
            Threads::Single(Single {
                state: State::new(frame_pool, event_proxy, config),
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
                    if let Err(err) = tx.try_send(event.clone()) {
                        log::error!("failed to send event to emulation thread: {event:?}. {err:?}");
                        std::process::exit(1);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn request_clock_frame(&mut self) -> NesResult<()> {
        // Multi-threaded emulation will handle frame clocking on its own
        if let Threads::Single(Single { ref mut state }) = self.threads {
            state.clock_frame();
        }
        Ok(())
    }
}
