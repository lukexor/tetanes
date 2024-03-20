//! User Interface representing the the NES Control Deck

use config::Config;
use emulation::Emulation;
use event::{NesEvent, State};
use platform::{BuilderExt, EventLoopExt, WindowExt};
use renderer::{BufferPool, Renderer};
use std::sync::Arc;
use tetanes_util::NesResult;
use winit::{
    event_loop::{EventLoop, EventLoopBuilder},
    window::{Fullscreen, Window, WindowBuilder},
};

pub mod action;
pub mod audio;
pub mod config;
pub mod emulation;
pub mod event;
pub mod input;
pub mod platform;
pub mod renderer;

/// Represents all the NES Emulation state.
#[derive(Debug)]
pub struct Nes {
    config: Config,
    window: Arc<Window>,
    emulation: Emulation,
    renderer: Renderer,
    #[cfg(target_arch = "wasm32")]
    event_proxy: winit::event_loop::EventLoopProxy<NesEvent>,
    state: State,
}

impl Nes {
    /// Begins emulation by starting the game engine loop.
    ///
    /// # Errors
    ///
    /// If engine fails to build or run, then an error is returned.
    pub async fn run(config: Config) -> NesResult<()> {
        // Set up window, events and NES state
        let event_loop = EventLoopBuilder::<NesEvent>::with_user_event().build()?;
        let mut nes = Nes::initialize(config, &event_loop).await?;
        event_loop
            .run_platform(move |event, window_target| nes.event_loop(event, window_target))?;

        Ok(())
    }

    /// Initializes the NES emulation.
    async fn initialize(config: Config, event_loop: &EventLoop<NesEvent>) -> NesResult<Self> {
        let window = Arc::new(Nes::initialize_window(event_loop, &config)?);
        let event_proxy = event_loop.create_proxy();
        let frame_pool = BufferPool::new();
        let state = State::new();
        let emulation =
            Emulation::initialize(event_proxy.clone(), frame_pool.clone(), config.clone())?;
        let renderer = Renderer::initialize(
            event_proxy.clone(),
            Arc::clone(&window),
            frame_pool,
            &config,
        )
        .await?;

        let mut nes = Self {
            config,
            window,
            emulation,
            renderer,
            #[cfg(target_arch = "wasm32")]
            event_proxy,
            state,
        };
        nes.initialize_platform()?;

        Ok(nes)
    }

    /// Initializes the window in a platform agnostic way.
    pub fn initialize_window(
        event_loop: &EventLoop<NesEvent>,
        config: &Config,
    ) -> NesResult<Window> {
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
        #[cfg(feature = "profiling")]
        puffin::GlobalProfiler::lock().new_frame();
        if let Err(err) = self.emulation.request_clock_frame() {
            self.on_error(err);
        }
    }
}
