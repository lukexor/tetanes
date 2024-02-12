//! User Interface representing the the NES Control Deck

use crate::{
    audio::Mixer,
    common::Regional,
    control_deck::ControlDeck,
    frame_begin,
    nes::{
        event::Event,
        menu::Menu,
        platform::{EventLoopExt, WindowBuilderExt},
        renderer::Renderer,
        state::Mode,
    },
    profile, NesResult,
};
use config::Config;
use std::io::Read;
use web_time::Instant;
use winit::{
    dpi::LogicalSize,
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Fullscreen, Window, WindowBuilder},
};

pub mod config;
pub mod event;
pub mod filesystem;
pub mod menu;
pub mod platform;
pub mod renderer;
pub mod replay;
pub mod rewind;
pub mod state;

/// Represents all the NES Emulation state.
#[derive(Debug)]
pub struct Nes {
    config: Config,
    window: Window,
    event_proxy: EventLoopProxy<Event>,
    control_deck: ControlDeck,
    // controllers: [Option<DeviceId>; 4],
    renderer: Renderer,
    mixer: Mixer,
    mode: Mode,
    last_frame_time: Instant,
    frame_accumulator: f32,
    messages: Vec<(String, Instant)>,
    error: Option<String>,
    event_state: event::State,
    rewind_state: rewind::State,
    replay_state: replay::State,
    // paths: Vec<PathBuf>,
    // selected_path: usize,
}

impl Nes {
    /// Begins emulation by starting the game engine loop.
    ///
    /// # Errors
    ///
    /// If engine fails to build or run, then an error is returned.
    pub async fn run(config: Config) -> NesResult<()> {
        // Set up window, events and NES state
        let event_loop = EventLoopBuilder::<Event>::with_user_event().build()?;
        let mut nes = Nes::initialize(config, &event_loop).await?;
        event_loop
            .run_platform(move |event, window_target| nes.handle_event(event, window_target))?;
        Ok(())
    }

    /// Initializes the NES emulation.
    async fn initialize(config: Config, event_loop: &EventLoop<Event>) -> NesResult<Self> {
        let window = Nes::initialize_window(event_loop, &config)?;
        let control_deck = ControlDeck::with_config(config.clone().into());
        let renderer = Renderer::initialize(&window, &config).await?;
        let sample_rate = config.audio_sample_rate / config.speed;
        let mixer = Mixer::new(
            control_deck.clock_rate() / sample_rate,
            sample_rate,
            config.audio_enabled,
        );

        let debug = config.debug;
        let mut nes = Self {
            config,
            window,
            event_proxy: event_loop.create_proxy(),
            control_deck,
            // controllers: [None; 4],
            renderer,
            mixer,
            mode: if debug { Mode::Pause } else { Mode::default() },
            messages: vec![],
            error: None,
            event_state: event::State::default(),
            rewind_state: rewind::State::default(),
            replay_state: replay::State::default(),
            // Keep track of last frame time so we can predict audio sync requirements for the next
            // frame.
            last_frame_time: Instant::now(),
            // A frame accumulator of partial frames for non-integer speed changes like
            // 1.5x.
            frame_accumulator: 0.0,
            //         paths: vec![],
            //         selected_path: 0,
        };

        nes.initialize_platform();
        Ok(nes)
    }

    /// Initializes the window in a platform agnostic way.
    pub fn initialize_window(event_loop: &EventLoop<Event>, config: &Config) -> NesResult<Window> {
        let (width, height) = config.get_dimensions();
        let window_builder = WindowBuilder::new();
        let window_builder = window_builder
            .with_active(true)
            .with_inner_size(LogicalSize::new(width, height))
            .with_title(Config::WINDOW_TITLE)
            // TODO: Support exclusive fullscreen config
            .with_fullscreen(config.fullscreen.then_some(Fullscreen::Borderless(None)))
            .with_platform();
        let window = window_builder.build(event_loop)?;

        if config.zapper {
            window.set_cursor_visible(false);
        }

        Ok(window)
    }

    fn draw_frame(&mut self) {
        if let Err(err) = self.renderer.draw_frame(self.control_deck.frame_buffer()) {
            self.handle_error(err);
        }
    }

    fn next_frame(&mut self, window_target: &EventLoopWindowTarget<Event>) {
        frame_begin!();
        profile!();

        if self.event_state.occluded {
            platform::sleep(self.config.target_frame_duration);
        } else {
            if self.replay_state.is_playing() {
                self.replay_action();
            }

            if self.is_playing() {
                // Frames that aren't multiples of the default render 1 more/less frames
                // every other frame
                // e.g. a speed of 1.5 will clock # of frames: 1, 2, 1, 2, 1, 2, 1, 2, ...
                // A speed of 0.5 will clock 0, 1, 0, 1, 0, 1, 0, 1, 0, ...
                self.frame_accumulator += self.config.speed;
                let mut frames_to_clock = 0;
                while self.frame_accumulator >= 1.0 {
                    self.frame_accumulator -= 1.0;
                    frames_to_clock += 1;
                }

                while self.mixer.queued_time() < self.config.audio_latency && frames_to_clock > 0 {
                    let now = Instant::now();
                    let last_frame_duration = now - self.last_frame_time;
                    self.last_frame_time = now;
                    log::trace!(
                        "last frame: {:.2}ms",
                        1000.0 * last_frame_duration.as_secs_f32(),
                    );

                    match self.control_deck.clock_frame() {
                        Ok(_) => {
                            self.update_rewind();
                            if let Err(err) = self.mixer.process(self.control_deck.audio_samples())
                            {
                                return self.handle_error(err);
                            }
                            self.control_deck.clear_audio_samples();
                        }
                        Err(err) => {
                            return self.handle_error(err);
                        }
                    }
                    frames_to_clock -= 1;
                }

                self.draw_frame();
                window_target.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + self.mixer.queued_time() - self.config.audio_latency,
                ));
            }
        }
    }
}
