//! User Interface representing the the NES Control Deck

use crate::{
    nes::{
        event::{RendererEvent, RunState, SendNesEvent, UiEvent},
        input::{Gamepads, InputBindings},
        renderer::{FrameRecycle, Resources},
    },
    platform::{EventLoopExt, Initialize},
    thread,
};
use anyhow::Context;
use config::Config;
use crossbeam::channel::{self, Receiver};
use egui::{ahash::HashMap, ViewportBuilder};
use egui_wgpu::winit::Painter;
use emulation::Emulation;
use event::NesEvent;
use renderer::Renderer;
use std::sync::Arc;
use tetanes_core::{time::Instant, video::Frame};
use thingbuf::mpsc::blocking;
use winit::{
    event::Modifiers,
    event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Window, WindowId},
};

pub mod action;
pub mod audio;
pub mod config;
pub mod emulation;
pub mod event;
pub mod input;
pub mod renderer;
pub mod rom;
pub mod version;

/// Represents all the NES Emulation state.
#[derive(Debug)]
#[must_use]
pub struct Nes {
    /// Set during initialization, then taken and set to `None` when running because
    /// `EventLoopProxy` can only be created on the initial `EventLoop` and not on
    /// `&EventLoopWindowTarget`.
    pub(crate) init_state: Option<(Config, EventLoopProxy<NesEvent>)>,
    /// Initially `Suspended`. `Pending` after `Resume` event received and spanwed. `Running` after
    /// resources future completes.
    pub(crate) state: State,
}

#[derive(Debug, Default)]
#[must_use]
pub(crate) enum State {
    #[default]
    Suspended,
    Pending {
        ctx: egui::Context,
        window: Arc<Window>,
        viewport_builder: ViewportBuilder,
        painter_rx: Receiver<Painter>,
    },
    Running(Running),
}

impl State {
    pub const fn is_suspended(&self) -> bool {
        matches!(self, Self::Suspended)
    }
}

/// Represents the NES running state.
#[derive(Debug)]
pub(crate) struct Running {
    pub(crate) cfg: Config,
    // Only used by wasm currently
    #[allow(unused)]
    pub(crate) tx: EventLoopProxy<NesEvent>,
    pub(crate) emulation: Emulation,
    pub(crate) renderer: Renderer,
    pub(crate) input_bindings: InputBindings,
    pub(crate) gamepads: Gamepads,
    pub(crate) modifiers: Modifiers,
    pub(crate) run_state: RunState,
    pub(crate) replay_recording: bool,
    pub(crate) audio_recording: bool,
    pub(crate) rewinding: bool,
    pub(crate) repaint_times: HashMap<WindowId, Instant>,
}

impl Nes {
    /// Runs the NES application by starting the event loop.
    ///
    /// # Errors
    ///
    /// If event loop fails to build or run, then an error is returned.
    pub fn run(cfg: Config) -> anyhow::Result<()> {
        // Set up window, events and NES state
        let event_loop = EventLoopBuilder::<NesEvent>::with_user_event().build()?;
        let mut nes = Nes::new(cfg, &event_loop);
        event_loop
            .run_platform(move |event, window_target| nes.event_loop(event, window_target))?;
        Ok(())
    }

    /// Create the NES instance.
    pub fn new(cfg: Config, event_loop: &EventLoop<NesEvent>) -> Self {
        let tx = event_loop.create_proxy();
        Self {
            init_state: Some((cfg, tx)),
            state: State::Suspended,
        }
    }

    pub(crate) fn request_resources(
        &mut self,
        event_loop: &EventLoopWindowTarget<NesEvent>,
    ) -> anyhow::Result<()> {
        let (cfg, tx) = self
            .init_state
            .as_ref()
            .context("config unexpectedly already taken")?;
        let ctx = egui::Context::default();
        let (window, viewport_builder) = Renderer::create_window(event_loop, &ctx, cfg)?;
        let window = Arc::new(window);

        let (painter_tx, painter_rx) = channel::bounded(1);
        thread::spawn({
            let window = Arc::clone(&window);
            let event_tx = tx.clone();
            async move {
                match Renderer::create_painter(window).await {
                    Ok(painter) => {
                        painter_tx.send(painter).expect("failed to send painter");
                        event_tx.nes_event(RendererEvent::ResourcesReady);
                    }
                    Err(err) => {
                        event_tx.nes_event(UiEvent::Error(format!(
                            "failed to create painter: {err:?}"
                        )));
                    }
                }
            }
        });

        self.state = State::Pending {
            ctx,
            window,
            viewport_builder,
            painter_rx,
        };

        Ok(())
    }

    /// Initialize the running state after a window and GPU resources are created. Transitions
    /// `state` from `Some(PendingGpuResources { .. })` to `Some(Running { .. })`.
    ///
    /// # Errors
    ///
    /// If GPU resources failed to be requested, the emulation or renderer fails to build, then an
    /// error is returned.
    pub(crate) fn init_running(
        &mut self,
        event_loop: &EventLoopWindowTarget<NesEvent>,
    ) -> anyhow::Result<()> {
        match std::mem::take(&mut self.state) {
            State::Pending {
                ctx,
                window,
                viewport_builder,
                painter_rx,
            } => {
                let resources = Resources {
                    ctx,
                    window,
                    viewport_builder,
                    painter: painter_rx.recv()?,
                };
                let (frame_tx, frame_rx) = blocking::with_recycle::<Frame, _>(3, FrameRecycle);
                let (mut cfg, tx) = self
                    .init_state
                    .take()
                    .context("config unexpectedly already taken")?;

                let input_bindings = InputBindings::from_input_config(&cfg.input);
                let gamepads = Gamepads::new();
                cfg.input.update_gamepad_assignments(&gamepads);

                let emulation = Emulation::new(tx.clone(), frame_tx.clone(), &cfg)?;
                let renderer = Renderer::new(tx.clone(), event_loop, resources, frame_rx, &cfg)?;

                let mut running = Running {
                    cfg,
                    tx,
                    emulation,
                    renderer,
                    input_bindings,
                    gamepads,
                    modifiers: Modifiers::default(),
                    run_state: RunState::Running,
                    replay_recording: false,
                    audio_recording: false,
                    rewinding: false,
                    repaint_times: HashMap::default(),
                };
                running.initialize()?;
                self.state = State::Running(running);
                Ok(())
            }
            State::Suspended => anyhow::bail!("not in pending state"),
            State::Running(running) => {
                self.state = State::Running(running);
                Ok(())
            }
        }
    }
}
