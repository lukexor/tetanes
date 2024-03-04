//! User Interface representing the the NES Control Deck

use crate::{
    frame_begin,
    nes::{
        config::Config,
        emulation::Emulation,
        event::State,
        platform::{BuilderExt, EventLoopExt, WindowExt},
        renderer::{BufferPool, Renderer},
    },
    profile, NesResult,
};
use std::sync::Arc;
use winit::{
    event_loop::EventLoop,
    window::{Fullscreen, Window, WindowBuilder},
};

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
    state: State,
    // controllers: [Option<DeviceId>; 4],
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
        let event_loop = EventLoop::new()?;
        let mut nes = Nes::initialize(config, &event_loop).await?;
        event_loop.run_platform(move |event, window_target| nes.on_event(event, window_target))?;

        Ok(())
    }

    /// Initializes the NES emulation.
    async fn initialize(config: Config, event_loop: &EventLoop<()>) -> NesResult<Self> {
        let window = Arc::new(Nes::initialize_window(event_loop, &config)?);
        let frame_pool = BufferPool::new();
        let state = State::new();
        let emulation =
            Emulation::initialize(state.tx.clone(), frame_pool.clone(), config.clone())?;
        let renderer =
            Renderer::initialize(state.tx.clone(), Arc::clone(&window), frame_pool, &config)
                .await?;

        let mut nes = Self {
            config,
            window,
            emulation,
            renderer,
            state,
        };
        nes.initialize_platform()?;

        Ok(nes)
    }

    /// Initializes the window in a platform agnostic way.
    pub fn initialize_window(event_loop: &EventLoop<()>, config: &Config) -> NesResult<Window> {
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
