use crate::{
    nes::{
        action::{Action, Feature, Setting, UiState},
        config::{Config, FrameSpeed, Scale},
        input::Input,
        renderer::gui::Menu,
        Nes,
    },
    platform::open_file_dialog,
};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tetanes_core::{
    action::Action as DeckAction,
    apu::Channel,
    common::{NesRegion, ResetKind},
    control_deck,
    input::{JoypadBtn, Player},
    time::Duration,
    video::VideoFilter,
};
use tracing::{error, trace};
use winit::{
    dpi::LogicalSize,
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
    RomLoaded((String, NesRegion)),
    RequestRedraw,
    ResizeWindow(LogicalSize<f32>),
    LoadRomDialog,
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
    Joypad((Player, JoypadBtn, ElementState)),
    LoadRomPath((std::path::PathBuf, Config)),
    LoadRom((String, RomData, Config)),
    TogglePause,
    Pause(bool),
    Reset(ResetKind),
    Rewind((ElementState, bool)),
    Screenshot,
    SetAudioEnabled(bool),
    SetFrameSpeed(FrameSpeed),
    SetRegion(NesRegion),
    SetTargetFrameDuration(Duration),
    StateLoad(control_deck::Config),
    StateSave(control_deck::Config),
    ToggleApuChannel(Channel),
    ToggleAudioRecord,
    ToggleReplayRecord,
    SetVideoFilter(VideoFilter),
    ZapperAim((u32, u32)),
    ZapperConnect(bool),
    ZapperTrigger,
}

#[derive(Debug, Clone)]
#[must_use]
pub enum RendererEvent {
    Frame(Duration),
    Menu(Menu),
    SetScale(Scale),
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
    pub modifiers: Modifiers,
    pub occluded: bool,
    pub paused: bool,
    pub quitting: bool,
}

impl State {
    pub fn new() -> Self {
        Self {
            modifiers: Modifiers::default(),
            occluded: false,
            paused: false,
            quitting: false,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl Nes {
    pub fn event_loop(
        &mut self,
        event: Event<NesEvent>,
        window_target: &EventLoopWindowTarget<NesEvent>,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.state.quitting {
            window_target.exit();
        }

        self.emulation.on_event(&event);
        self.renderer.on_event(&self.window, &event);

        match event {
            Event::WindowEvent {
                window_id, event, ..
            } => {
                match event {
                    WindowEvent::CloseRequested => {
                        if window_id == self.window.id() {
                            window_target.exit();
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if let Err(err) =
                            self.renderer.request_redraw(&self.window, &mut self.config)
                        {
                            self.on_error(err);
                        }
                        self.window.request_redraw();
                    }
                    WindowEvent::Occluded(occluded) => {
                        if window_id == self.window.id() {
                            self.state.occluded = occluded;
                            // Don't unpause if paused manually
                            if !self.state.paused {
                                self.trigger_event(EmulationEvent::Pause(self.state.occluded));
                            }
                            if self.state.occluded {
                                window_target.set_control_flow(ControlFlow::Wait);
                            } else {
                                window_target.set_control_flow(ControlFlow::Poll);
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
                        self.trigger_event(EmulationEvent::LoadRomPath((
                            path,
                            self.config.clone(),
                        )));
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
                if let Err(err) = self.config.save() {
                    error!("{err:?}");
                }
            }
            Event::UserEvent(NesEvent::Ui(event)) => self.on_event(event),
            _ => (),
        }
    }

    pub fn on_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Message(msg) => self.add_message(msg),
            UiEvent::Error(err) => self.on_error(anyhow!(err)),
            UiEvent::Terminate => self.state.quitting = true,
            UiEvent::RomLoaded((name, region)) => {
                self.window.set_title(&name);
                self.config.set_region(region);
            }
            UiEvent::ResizeWindow(size) => {
                let _ = self.window.request_inner_size(size);
                self.window.set_min_inner_size(Some(size));
            }
            UiEvent::RequestRedraw => self.window.request_redraw(),
            UiEvent::LoadRomDialog => match open_file_dialog("NES ROMs", &["nes"]) {
                Ok(maybe_path) => {
                    if let Some(path) = maybe_path {
                        self.trigger_event(EmulationEvent::LoadRomPath((
                            path,
                            self.config.clone(),
                        )));
                    }
                }
                Err(err) => {
                    error!("failed to open rom dialog: {err:?}");
                    self.trigger_event(UiEvent::Error("failed to open rom dialog".to_string()))
                }
            },
        }
    }

    /// Trigger a custom event.
    pub fn trigger_event(&mut self, event: impl Into<NesEvent>) {
        let event = event.into();
        trace!("Nes event: {event:?}");

        match event {
            NesEvent::Ui(event) => self.on_event(event),
            NesEvent::Emulation(ref emulation_event) => {
                if let EmulationEvent::LoadRomPath((path, ..)) = emulation_event {
                    self.config.recent_roms.insert(path.clone());
                }
                self.emulation.on_event(&Event::UserEvent(event));
            }
            NesEvent::Renderer(_) => self
                .renderer
                .on_event(&self.window, &Event::UserEvent(event)),
        }
    }

    /// Handle user input mapped to key bindings.
    pub fn on_input(&mut self, input: Input, state: ElementState, repeat: bool) {
        if let Some((player, action)) = self.config.input_map.get(&input).copied() {
            trace!("player: {player:?}, action: {action:?}, state: {state:?}, repeat: {repeat:?}");
            let released = state == ElementState::Released;
            match action {
                Action::Ui(state) if released => match state {
                    UiState::Quit => self.trigger_event(UiEvent::Terminate),
                    UiState::TogglePause => {
                        self.state.paused = !self.state.paused;
                        self.trigger_event(EmulationEvent::TogglePause);
                    }
                },
                Action::Menu(menu) if released => self.trigger_event(RendererEvent::Menu(menu)),
                Action::Feature(feature) => match feature {
                    Feature::ToggleReplayRecord if released => {
                        self.trigger_event(EmulationEvent::ToggleReplayRecord);
                    }
                    Feature::ToggleAudioRecord if released => {
                        self.trigger_event(EmulationEvent::ToggleAudioRecord);
                    }
                    Feature::TakeScreenshot if released => {
                        self.trigger_event(EmulationEvent::Screenshot)
                    }
                    Feature::Rewind => self.trigger_event(EmulationEvent::Rewind((state, repeat))),
                    _ => (),
                },
                Action::Setting(setting) => match setting {
                    Setting::ToggleFullscreen if released => {
                        self.config.fullscreen = !self.config.fullscreen;
                        self.window.set_fullscreen(
                            self.config
                                .fullscreen
                                .then_some(Fullscreen::Borderless(None)),
                        );
                    }
                    Setting::ToggleVsync if released => {
                        self.config.vsync = !self.config.vsync;
                        self.trigger_event(RendererEvent::SetVSync(self.config.vsync));
                    }
                    Setting::ToggleAudio if released => {
                        self.config.audio_enabled = !self.config.audio_enabled;
                        self.trigger_event(EmulationEvent::SetAudioEnabled(
                            self.config.audio_enabled,
                        ));
                    }
                    Setting::IncSpeed if released => {
                        self.config.frame_speed = self.config.frame_speed.increment();
                        self.set_speed(self.config.frame_speed);
                    }
                    Setting::DecSpeed if released => {
                        self.config.frame_speed = self.config.frame_speed.decrement();
                        self.set_speed(self.config.frame_speed);
                    }
                    Setting::FastForward if !repeat => self.set_speed(if released {
                        FrameSpeed::default()
                    } else {
                        FrameSpeed::X200
                    }),
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
                        if !self.config.concurrent_dpad && pressed {
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
                    DeckAction::ZapperTrigger if self.config.deck.zapper => {
                        self.trigger_event(EmulationEvent::ZapperTrigger);
                    }
                    DeckAction::SetSaveSlot(slot) if released => {
                        self.config.deck.save_slot = slot;
                        self.add_message(format!("Changed Save Slot to {slot}"));
                    }
                    DeckAction::SaveState if released => {
                        self.trigger_event(EmulationEvent::StateSave(self.config.deck.clone()));
                    }
                    DeckAction::LoadState if released => {
                        self.trigger_event(EmulationEvent::StateLoad(self.config.deck.clone()));
                    }
                    DeckAction::ToggleApuChannel(channel) if released => {
                        self.trigger_event(EmulationEvent::ToggleApuChannel(channel));
                    }
                    DeckAction::MapperRevision(_) if released => todo!("mapper revision"),
                    DeckAction::SetNesRegion(region) if released => {
                        self.trigger_event(EmulationEvent::SetRegion(region));
                    }
                    DeckAction::SetVideoFilter(filter) if released => {
                        self.config.deck.filter = if self.config.deck.filter == filter {
                            VideoFilter::Pixellate
                        } else {
                            filter
                        };
                        self.trigger_event(EmulationEvent::SetVideoFilter(self.config.deck.filter));
                    }
                    _ => (),
                },
                _ => (),
            }
        }
    }
}
