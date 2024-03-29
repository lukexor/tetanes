use crate::{
    nes::{
        action::{Action, Feature, Setting, UiState},
        config::Config,
        input::{Input, InputMap},
        renderer::gui::Menu,
        Nes,
    },
    platform::open_file_dialog,
};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tetanes_core::{
    action::Action as DeckAction,
    apu::Channel,
    common::{NesRegion, ResetKind},
    input::{FourPlayer, JoypadBtn, Player},
    video::VideoFilter,
};
use tracing::{error, trace};
use winit::{
    event::{ElementState, Event, Modifiers, WindowEvent},
    event_loop::{ControlFlow, EventLoopWindowTarget},
    keyboard::PhysicalKey,
    window::Fullscreen,
};

#[derive(Debug, Clone)]
#[must_use]
pub enum UiEvent {
    Error(String),
    Message(String),
    RequestRedraw,
    LoadRomDialog,
    LoadReplayDialog,
    Terminate,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RomData(Vec<u8>);

impl std::fmt::Debug for RomData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RomData({} bytes)", self.0.len())
    }
}

impl AsRef<[u8]> for RomData {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl RomData {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub enum EmulationEvent {
    InstantRewind,
    Joypad((Player, JoypadBtn, ElementState)),
    LoadRom((String, RomData)),
    LoadRomPath(PathBuf),
    LoadReplayPath(PathBuf),
    Pause(bool),
    Reset(ResetKind),
    Rewind(bool),
    Screenshot,
    SetAudioEnabled(bool),
    SetCycleAccurate(bool),
    SetFourPlayer(FourPlayer),
    SetSpeed(f32),
    SetRegion(NesRegion),
    SetVideoFilter(VideoFilter),
    StateLoad,
    StateSave,
    ToggleApuChannel(Channel),
    AudioRecord(bool),
    ReplayRecord(bool),
    ZapperAim((u32, u32)),
    ZapperConnect(bool),
    ZapperTrigger,
}

#[derive(Debug, Clone)]
#[must_use]
pub enum RendererEvent {
    Frame,
    RomLoaded(String),
    Menu(Menu),
    SetVSync(bool),
}

#[derive(Debug, Clone)]
#[must_use]
pub enum NesEvent {
    Ui(UiEvent),
    Emulation(EmulationEvent),
    Renderer(RendererEvent),
}

impl From<UiEvent> for NesEvent {
    fn from(event: UiEvent) -> Self {
        Self::Ui(event)
    }
}

impl From<EmulationEvent> for NesEvent {
    fn from(event: EmulationEvent) -> Self {
        Self::Emulation(event)
    }
}

impl From<RendererEvent> for NesEvent {
    fn from(event: RendererEvent) -> Self {
        Self::Renderer(event)
    }
}

#[derive(Debug)]
#[must_use]
pub struct State {
    pub input_map: InputMap,
    pub modifiers: Modifiers,
    pub occluded: bool,
    pub paused: bool,
    pub replay_recording: bool,
    pub audio_recording: bool,
    pub rewinding: bool,
    pub quitting: bool,
}

impl State {
    pub fn new(config: &Config) -> Self {
        Self {
            input_map: config.read(|cfg| InputMap::from_bindings(&cfg.input.bindings)),
            modifiers: Modifiers::default(),
            occluded: false,
            paused: false,
            replay_recording: false,
            audio_recording: false,
            rewinding: false,
            quitting: false,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Nes {
    pub fn event_loop(
        &mut self,
        event: Event<NesEvent>,
        event_loop: &EventLoopWindowTarget<NesEvent>,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.state.quitting {
            event_loop.exit();
        }
        if self.state.occluded {
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            event_loop.set_control_flow(ControlFlow::Poll);
        }

        self.renderer.on_event(&self.window, &event);

        match event {
            Event::WindowEvent {
                window_id, event, ..
            } => {
                match event {
                    WindowEvent::CloseRequested => {
                        if window_id == self.window.id() {
                            event_loop.exit();
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if !self.state.occluded {
                            if let Err(err) = self.renderer.request_redraw(&self.window) {
                                self.on_error(err);
                            }
                            self.window.request_redraw();
                        }
                    }
                    WindowEvent::Occluded(occluded) => {
                        if window_id == self.window.id() {
                            self.state.occluded = occluded;
                            // Don't unpause if paused manually
                            if !self.state.paused {
                                self.trigger_event(EmulationEvent::Pause(self.state.occluded));
                            }
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if let PhysicalKey::Code(key) = event.physical_key {
                            self.on_input(
                                Input::Key(key, self.state.modifiers.state()),
                                event.state,
                                event.repeat,
                            );
                        }
                    }
                    WindowEvent::ModifiersChanged(modifiers) => self.state.modifiers = modifiers,
                    WindowEvent::MouseInput { button, state, .. } => {
                        self.on_input(Input::Mouse(button, state), state, false);
                    }
                    WindowEvent::DroppedFile(path) => {
                        self.trigger_event(EmulationEvent::LoadRomPath(path));
                    }
                    WindowEvent::HoveredFile(_) => (), // TODO: Show file drop cursor
                    WindowEvent::HoveredFileCancelled => (), // TODO: Restore cursor
                    _ => (),
                }
            }
            Event::AboutToWait => self.next_frame(),
            Event::LoopExiting => {
                #[cfg(feature = "profiling")]
                puffin::set_scopes_on(false);
                if let Err(err) = self.config.read(|cfg| cfg.save()) {
                    error!("{err:?}");
                }
            }
            Event::UserEvent(NesEvent::Emulation(event)) => self.emulation.on_event(&event),
            Event::UserEvent(NesEvent::Ui(event)) => self.on_event(event),
            _ => (),
        }
    }

    pub fn on_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Message(msg) => self.add_message(msg),
            UiEvent::Error(err) => self.on_error(anyhow!(err)),
            UiEvent::Terminate => self.state.quitting = true,
            UiEvent::RequestRedraw => self.window.request_redraw(),
            UiEvent::LoadRomDialog => match open_file_dialog(
                "Load ROM",
                "NES ROMs",
                &["nes"],
                self.config
                    .read(|cfg| cfg.renderer.roms_path.as_ref().map(|p| p.to_path_buf())),
            ) {
                Ok(maybe_path) => {
                    if let Some(path) = maybe_path {
                        self.trigger_event(EmulationEvent::LoadRomPath(path));
                    }
                }
                Err(err) => {
                    error!("failed top open rom dialog: {err:?}");
                    self.trigger_event(UiEvent::Error("failed to open rom dialog".to_string()));
                }
            },
            UiEvent::LoadReplayDialog => {
                match open_file_dialog(
                    "Load Replay",
                    "Replay Recording",
                    &["replay"],
                    Config::document_dir(),
                ) {
                    Ok(maybe_path) => {
                        if let Some(path) = maybe_path {
                            self.trigger_event(EmulationEvent::LoadReplayPath(path));
                        }
                    }
                    Err(err) => {
                        error!("failed top open replay dialog: {err:?}");
                        self.trigger_event(UiEvent::Error(
                            "failed to open replay dialog".to_string(),
                        ));
                    }
                }
            }
        }
    }

    /// Trigger a custom event.
    pub fn trigger_event(&mut self, event: impl Into<NesEvent>) {
        let event = event.into();
        trace!("Nes event: {event:?}");

        match event {
            NesEvent::Ui(event) => self.on_event(event),
            NesEvent::Emulation(ref event) => {
                if let EmulationEvent::LoadRomPath(path) = event {
                    if let Ok(path) = path.canonicalize() {
                        self.config
                            .write(|cfg| cfg.renderer.recent_roms.insert(path));
                    }
                }
                self.emulation.on_event(event);
            }
            NesEvent::Renderer(_) => self
                .renderer
                .on_event(&self.window, &Event::UserEvent(event)),
        }
    }

    /// Handle user input mapped to key bindings.
    pub fn on_input(&mut self, input: Input, state: ElementState, repeat: bool) {
        if let Some((player, action)) = self.state.input_map.get(&input).copied() {
            trace!("player: {player:?}, action: {action:?}, state: {state:?}, repeat: {repeat:?}");
            let released = state == ElementState::Released;
            match action {
                Action::Ui(state) if released => match state {
                    UiState::Quit => self.trigger_event(UiEvent::Terminate),
                    UiState::TogglePause => {
                        self.state.paused = !self.state.paused;
                        self.trigger_event(EmulationEvent::Pause(self.state.paused));
                    }
                    UiState::LoadRom => {
                        self.state.paused = !self.state.paused;
                        self.trigger_event(EmulationEvent::Pause(self.state.paused));
                        self.trigger_event(UiEvent::LoadRomDialog);
                    }
                },
                Action::Menu(menu) if released => self.trigger_event(RendererEvent::Menu(menu)),
                Action::Feature(feature) => match feature {
                    Feature::ToggleReplayRecord if released => {
                        self.state.replay_recording = !self.state.replay_recording;
                        self.trigger_event(EmulationEvent::ReplayRecord(
                            self.state.replay_recording,
                        ));
                    }
                    Feature::ToggleAudioRecord if released => {
                        self.state.audio_recording = !self.state.audio_recording;
                        self.trigger_event(EmulationEvent::AudioRecord(self.state.audio_recording));
                    }
                    Feature::TakeScreenshot if released => {
                        self.trigger_event(EmulationEvent::Screenshot);
                    }
                    Feature::Rewind => {
                        if !self.state.rewinding {
                            if repeat {
                                self.state.rewinding = true;
                                self.trigger_event(EmulationEvent::Rewind(self.state.rewinding));
                            } else if released {
                                self.trigger_event(EmulationEvent::InstantRewind);
                            }
                        } else if released {
                            self.state.rewinding = false;
                            self.trigger_event(EmulationEvent::Rewind(self.state.rewinding));
                        }
                    }
                    _ => (),
                },
                Action::Setting(setting) => match setting {
                    Setting::ToggleFullscreen if released => {
                        let fullscreen = self.config.write(|cfg| {
                            cfg.renderer.fullscreen = !cfg.renderer.fullscreen;
                            cfg.renderer.fullscreen
                        });
                        self.window
                            .set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
                    }
                    Setting::ToggleVsync if released => {
                        let vsync = self.config.write(|cfg| {
                            cfg.renderer.vsync = !cfg.renderer.vsync;
                            cfg.renderer.vsync
                        });
                        self.trigger_event(RendererEvent::SetVSync(vsync));
                    }
                    Setting::ToggleAudio if released => {
                        let enabled = self.config.write(|cfg| {
                            cfg.audio.enabled = !cfg.audio.enabled;
                            cfg.audio.enabled
                        });
                        self.trigger_event(EmulationEvent::SetAudioEnabled(enabled));
                    }
                    Setting::ToggleMenuBar if released => {
                        self.config.write(|cfg| {
                            cfg.renderer.show_menubar = !cfg.renderer.show_menubar;
                        });
                    }
                    Setting::IncSpeed if released => {
                        if self.config.read(|cfg| cfg.emulation.speed <= 1.75) {
                            let speed = self.config.write(|cfg| {
                                cfg.emulation.speed += 0.25;
                                cfg.emulation.speed
                            });
                            self.set_speed(speed);
                        }
                    }
                    Setting::DecSpeed if released => {
                        if self.config.read(|cfg| cfg.emulation.speed >= 0.25) {
                            let speed = self.config.write(|cfg| {
                                cfg.emulation.speed -= 0.25;
                                cfg.emulation.speed
                            });
                            self.set_speed(speed);
                        }
                    }
                    Setting::FastForward if !repeat => {
                        self.set_speed(if released { 1.0 } else { 2.0 });
                    }
                    _ => (),
                },
                Action::Deck(action) => match action {
                    DeckAction::SoftReset if released => {
                        self.trigger_event(EmulationEvent::Reset(ResetKind::Soft));
                    }
                    DeckAction::HardReset if released => {
                        self.trigger_event(EmulationEvent::Reset(ResetKind::Hard));
                    }
                    DeckAction::Joypad(button) if !repeat => {
                        let pressed = state == ElementState::Pressed;
                        if pressed && !self.config.read(|cfg| cfg.deck.concurrent_dpad) {
                            if let Some(button) = match button {
                                JoypadBtn::Left => Some(JoypadBtn::Right),
                                JoypadBtn::Right => Some(JoypadBtn::Left),
                                JoypadBtn::Up => Some(JoypadBtn::Down),
                                JoypadBtn::Down => Some(JoypadBtn::Up),
                                _ => None,
                            } {
                                self.trigger_event(EmulationEvent::Joypad((
                                    player,
                                    button,
                                    ElementState::Released,
                                )));
                            }
                        }
                        self.trigger_event(EmulationEvent::Joypad((player, button, state)));
                    }
                    DeckAction::ZapperTrigger => {
                        if self.config.read(|cfg| cfg.deck.zapper) {
                            self.trigger_event(EmulationEvent::ZapperTrigger);
                        }
                    }
                    DeckAction::SetSaveSlot(slot) if released => {
                        if self.config.read(|cfg| cfg.emulation.save_slot != slot) {
                            self.config.write(|cfg| cfg.emulation.save_slot = slot);
                            self.add_message(format!("Changed Save Slot to {slot}"));
                        }
                    }
                    DeckAction::SaveState if released => {
                        self.trigger_event(EmulationEvent::StateSave);
                    }
                    DeckAction::LoadState if released => {
                        self.trigger_event(EmulationEvent::StateLoad);
                    }
                    DeckAction::ToggleApuChannel(channel) if released => {
                        self.trigger_event(EmulationEvent::ToggleApuChannel(channel));
                    }
                    DeckAction::MapperRevision(_) if released => todo!("mapper revision"),
                    DeckAction::SetNesRegion(region) if released => {
                        self.trigger_event(EmulationEvent::SetRegion(region));
                    }
                    DeckAction::SetVideoFilter(filter) if released => {
                        let filter = self.config.write(|cfg| {
                            cfg.deck.filter = if cfg.deck.filter == filter {
                                VideoFilter::Pixellate
                            } else {
                                filter
                            };
                            cfg.deck.filter
                        });
                        self.trigger_event(EmulationEvent::SetVideoFilter(filter));
                    }
                    _ => (),
                },
                _ => (),
            }
        }
    }
}
