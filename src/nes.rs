//! User Interface representing the the NES Control Deck

use crate::{
    frame_begin,
    nes::{
        emulation::Emulation,
        event::Event,
        platform::{EventLoopExt, WindowBuilderExt, WindowExt},
        renderer::{BufferPool, Renderer},
    },
    profile, NesResult,
};
use config::Config;
use std::sync::Arc;
use winit::{
    event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy},
    window::{Fullscreen, Window, WindowBuilder},
};

pub mod config;
pub mod emulation;
pub mod event;
pub mod gui;
pub mod platform;
pub mod renderer;

/// Represents all the NES Emulation state.
#[derive(Debug)]
pub struct Nes {
    config: Config,
    window: Arc<Window>,
    #[allow(unused)]
    event_proxy: EventLoopProxy<Event>,
    emulation: Emulation,
    // controllers: [Option<DeviceId>; 4],
    renderer: Renderer,
    event_state: event::State,
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
        event_loop.run_platform(move |event, window_target| nes.on_event(event, window_target))?;
        Ok(())
    }

    /// Initializes the NES emulation.
    async fn initialize(config: Config, event_loop: &EventLoop<Event>) -> NesResult<Self> {
        let window = Arc::new(Nes::initialize_window(event_loop, &config)?);
        let frame_pool = BufferPool::new();
        let emulation = Emulation::initialize(
            event_loop,
            Arc::clone(&window),
            frame_pool.clone(),
            config.clone(),
        )?;
        let renderer =
            Renderer::initialize(event_loop, Arc::clone(&window), frame_pool, &config).await?;

        let mut nes = Self {
            config,
            window,
            event_proxy: event_loop.create_proxy(),
            emulation,
            // controllers: [None; 4],
            renderer,
            event_state: event::State::default(),
        };

        nes.initialize_platform();
        Ok(nes)
    }

    /// Initializes the window in a platform agnostic way.
    pub fn initialize_window(event_loop: &EventLoop<Event>, config: &Config) -> NesResult<Window> {
        let (inner_size, min_inner_size) = config.inner_dimensions();
        let window_builder = WindowBuilder::new();
        let window_builder = window_builder
            .with_active(true)
            .with_inner_size(inner_size)
            .with_min_inner_size(min_inner_size)
            .with_title(Config::WINDOW_TITLE)
            // TODO: Support exclusive fullscreen config
            .with_fullscreen(config.fullscreen.then_some(Fullscreen::Borderless(None)))
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
        self.window.request_redraw();
    }
}
