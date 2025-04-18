//! User Interface representing the the NES Control Deck

use crate::{
    nes::{
        emulation::Emulation,
        event::{NesEvent, NesEventProxy},
        input::{Gamepads, InputBindings},
        renderer::{FrameRecycle, Renderer, Resources, painter::Painter},
    },
    platform::Initialize,
};
use anyhow::Context;
use cfg_if::cfg_if;
use config::Config;
use crossbeam::channel::Receiver;
use egui::ahash::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tetanes_core::{time::Instant, video::Frame};
use thingbuf::mpsc::blocking;
use winit::{
    event::Modifiers,
    event_loop::{ActiveEventLoop, EventLoop},
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
    pub(crate) init_state: Option<(Config, NesEventProxy)>,
    /// Initially `Suspended`. `Pending` after `Resume` event received and spanwed. `Running` after
    /// resources future completes.
    pub(crate) state: State,
}

#[derive(Debug)]
#[must_use]
pub(crate) enum State {
    Suspended {
        should_terminate: Arc<AtomicBool>,
    },
    Pending {
        ctx: egui::Context,
        window: Arc<Window>,
        painter_rx: Receiver<Painter>,
        should_terminate: Arc<AtomicBool>,
    },
    Running(Box<Running>),
    Exiting,
}

impl Default for State {
    fn default() -> Self {
        Self::Suspended {
            should_terminate: Default::default(),
        }
    }
}

impl State {
    pub const fn is_exiting(&self) -> bool {
        matches!(self, Self::Exiting)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[must_use]
pub enum RunState {
    Running,
    ManuallyPaused,
    AutoPaused,
}

impl RunState {
    pub const fn paused(&self) -> bool {
        matches!(self, Self::ManuallyPaused | Self::AutoPaused)
    }

    pub const fn auto_paused(&self) -> bool {
        matches!(self, Self::AutoPaused)
    }

    pub const fn manually_paused(&self) -> bool {
        matches!(self, Self::ManuallyPaused)
    }
}

/// Represents the NES running state.
#[derive(Debug)]
pub(crate) struct Running {
    pub(crate) cfg: Config,
    // Only used by wasm currently
    #[cfg_attr(target_arch = "wasm32", allow(unused))]
    pub(crate) tx: NesEventProxy,
    pub(crate) should_terminate: Arc<AtomicBool>,
    pub(crate) emulation: Emulation,
    pub(crate) renderer: Renderer,
    pub(crate) input_bindings: InputBindings,
    pub(crate) gamepads: Gamepads,
    pub(crate) modifiers: Modifiers,
    pub(crate) replay_recording: bool,
    pub(crate) audio_recording: bool,
    pub(crate) rewinding: bool,
    pub(crate) occluded: bool,
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
        let event_loop = EventLoop::<NesEvent>::with_user_event().build()?;
        let nes = Nes::new(cfg, &event_loop);
        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                use winit::platform::web::EventLoopExtWebSys;
                event_loop.spawn_app(nes);
            } else {
                let mut nes = nes;
                event_loop.run_app(&mut nes)?;
            }
        }
        Ok(())
    }

    /// Return whether the application should terminate.
    pub fn should_terminate(&self) -> bool {
        match &self.state {
            State::Suspended { should_terminate }
            | State::Pending {
                should_terminate, ..
            } => should_terminate.load(Ordering::Relaxed),
            State::Running(running) => running.should_terminate.load(Ordering::Relaxed),
            State::Exiting => true,
        }
    }

    /// Create the NES instance.
    pub fn new(cfg: Config, event_loop: &EventLoop<NesEvent>) -> Self {
        let should_terminate = Arc::new(AtomicBool::new(false));
        #[cfg(not(target_arch = "wasm32"))]
        // Minor issue if this fails, but not enough to terminate the program
        let _ = ctrlc::set_handler({
            let should_terminate = Arc::clone(&should_terminate);
            move || {
                should_terminate.store(true, Ordering::Relaxed);
            }
        });

        Self {
            init_state: Some((cfg, NesEventProxy::new(event_loop))),
            state: State::Suspended { should_terminate },
        }
    }

    /// Request renderer resources (creating gui context, window, painter, etc).
    ///
    /// # Errors
    ///
    /// Returns an error if any resources can't be created correctly or `init_running` has already
    /// been called.
    pub(crate) fn request_renderer_resources(
        &mut self,
        event_loop: &ActiveEventLoop,
        should_terminate: Arc<AtomicBool>,
    ) -> anyhow::Result<()> {
        let (cfg, tx) = self
            .init_state
            .as_ref()
            .context("config unexpectedly already taken")?;

        let (ctx, window, painter_rx) = Renderer::request_resources(event_loop, tx, cfg)?;

        self.state = State::Pending {
            ctx,
            window,
            painter_rx,
            should_terminate,
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
    pub(crate) fn init_running(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        match std::mem::take(&mut self.state) {
            State::Pending {
                ctx,
                window,
                painter_rx,
                should_terminate,
            } => {
                let resources = Resources {
                    ctx,
                    window,
                    painter: painter_rx.recv()?,
                };
                let (frame_tx, frame_rx) = blocking::with_recycle::<Frame, _>(10, FrameRecycle);
                let (mut cfg, tx) = self
                    .init_state
                    .take()
                    .context("config unexpectedly already taken")?;

                let input_bindings = InputBindings::from_input_config(&cfg.input);
                let gamepads = Gamepads::new();
                cfg.input.update_gamepad_assignments(&gamepads);

                let emulation = Emulation::new(tx.clone(), frame_tx.clone(), &cfg)?;
                let renderer = Renderer::new(event_loop, tx.clone(), resources, frame_rx, &cfg)?;

                let mut running = Running {
                    cfg,
                    tx,
                    should_terminate,
                    emulation,
                    renderer,
                    input_bindings,
                    gamepads,
                    modifiers: Modifiers::default(),
                    replay_recording: false,
                    audio_recording: false,
                    rewinding: false,
                    occluded: false,
                    repaint_times: HashMap::default(),
                };
                running.initialize()?;
                self.state = State::Running(Box::new(running));
                Ok(())
            }
            State::Running(running) => {
                self.state = State::Running(running);
                Ok(())
            }
            State::Suspended { .. } | State::Exiting => anyhow::bail!("not in pending state"),
        }
    }
}
