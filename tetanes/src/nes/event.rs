use crate::{
    nes::{
        action::{Action, Debug, DebugStep, Feature, Setting, Ui},
        config::Config,
        emulation::FrameStats,
        input::{Input, InputBindings},
        renderer::gui::Menu,
        Nes,
    },
    platform::{self, open_file_dialog},
};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tetanes_core::{
    action::Action as DeckAction,
    apu::Channel,
    common::{NesRegion, ResetKind},
    control_deck::MapperRevisions,
    genie::GenieCode,
    input::{FourPlayer, JoypadBtn, Player},
    mem::RamState,
    time::Duration,
    video::VideoFilter,
};
use tracing::{error, trace};
use winit::{
    event::{ElementState, Event, Modifiers, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopProxy, EventLoopWindowTarget},
    keyboard::PhysicalKey,
    window::Fullscreen,
};

pub trait SendNesEvent {
    fn nes_event(&mut self, event: impl Into<NesEvent>);
}

impl SendNesEvent for EventLoopProxy<NesEvent> {
    fn nes_event(&mut self, event: impl Into<NesEvent>) {
        let event = event.into();
        trace!("sending event: {event:?}");
        if let Err(err) = self.send_event(event) {
            error!("failed to send event: {err:?}");
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum UiEvent {
    Error(String),
    Message(String),
    IgnoreInputActions(bool),
    LoadRomDialog,
    LoadReplayDialog,
    Terminate,
}

#[derive(Clone, PartialEq)]
pub struct RomData(pub Vec<u8>);

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

#[derive(Clone, PartialEq)]
pub struct ReplayData(pub Vec<u8>);

impl std::fmt::Debug for ReplayData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ReplayData({} bytes)", self.0.len())
    }
}

impl AsRef<[u8]> for ReplayData {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum ConfigEvent {
    ApuChannelEnabled((Channel, bool)),
    AudioBuffer(usize),
    AudioEnabled(bool),
    AudioLatency(Duration),
    AutoLoad(bool),
    AutoSave(bool),
    ConcurrentDpad(bool),
    CycleAccurate(bool),
    FourPlayer(FourPlayer),
    Fullscreen(bool),
    GenieCodeAdded(GenieCode),
    GenieCodeRemoved(String),
    HideOverscan(bool),
    InputBindings,
    MapperRevisions(MapperRevisions),
    RamState(RamState),
    Region(NesRegion),
    RewindEnabled(bool),
    RunAhead(usize),
    SaveSlot(u8),
    Scale(f32),
    Speed(f32),
    VideoFilter(VideoFilter),
    Vsync(bool),
    ZapperConnected(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum EmulationEvent {
    AudioRecord(bool),
    DebugStep(DebugStep),
    InstantRewind,
    Joypad((Player, JoypadBtn, ElementState)),
    #[serde(skip)]
    LoadReplay((String, ReplayData)),
    LoadReplayPath(PathBuf),
    #[serde(skip)]
    LoadRom((String, RomData)),
    LoadRomPath(PathBuf),
    LoadState(u8),
    Occluded(bool),
    Pause(bool),
    ReplayRecord(bool),
    Reset(ResetKind),
    Rewinding(bool),
    SaveState(u8),
    Screenshot,
    UnloadRom,
    ZapperAim((u32, u32)),
    ZapperTrigger,
}

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum RendererEvent {
    FrameStats(FrameStats),
    ScaleChanged,
    RequestRedraw(Duration),
    RomLoaded((String, NesRegion)),
    RomUnloaded,
    Menu(Menu),
}

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum NesEvent {
    Ui(UiEvent),
    Emulation(EmulationEvent),
    Renderer(RendererEvent),
    Config(ConfigEvent),
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

impl From<ConfigEvent> for NesEvent {
    fn from(event: ConfigEvent) -> Self {
        Self::Config(event)
    }
}

#[derive(Debug)]
#[must_use]
pub struct State {
    pub input_bindings: InputBindings,
    pub pending_keybind: bool,
    pub modifiers: Modifiers,
    pub occluded: bool,
    pub paused: bool,
    pub replay_recording: bool,
    pub audio_recording: bool,
    pub rewinding: bool,
}

impl State {
    pub fn new(cfg: &Config) -> Self {
        Self {
            input_bindings: InputBindings::from_input_config(&cfg.input),
            pending_keybind: false,
            modifiers: Modifiers::default(),
            occluded: false,
            paused: false,
            replay_recording: false,
            audio_recording: false,
            rewinding: false,
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

        let mut repaint = false;
        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                repaint = true;
            }
            Event::AboutToWait => {
                #[cfg(feature = "profiling")]
                puffin::GlobalProfiler::lock().new_frame();
                self.emulation.clock_frame();
            }
            Event::WindowEvent {
                window_id, event, ..
            } => {
                self.renderer.on_window_event(&self.window, &event);

                match event {
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        if window_id == self.window.id() {
                            event_loop.exit();
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if !self.state.occluded {
                            if let Err(err) = self.renderer.request_redraw(
                                &self.window,
                                event_loop,
                                &mut self.cfg,
                            ) {
                                self.on_error(err);
                            }
                        }
                    }
                    WindowEvent::Occluded(occluded) => {
                        if window_id == self.window.id() {
                            self.state.occluded = occluded;
                            self.nes_event(EmulationEvent::Occluded(self.state.occluded));
                            event_loop.set_control_flow(if occluded {
                                ControlFlow::Wait
                            } else {
                                ControlFlow::Poll
                            });
                        }
                        if !occluded {
                            repaint = true;
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        repaint = true;
                        if !self.state.pending_keybind {
                            if let PhysicalKey::Code(key) = event.physical_key {
                                self.on_input(
                                    Input::Key(key, self.state.modifiers.state()),
                                    event.state,
                                    event.repeat,
                                );
                            }
                        }
                    }
                    WindowEvent::ModifiersChanged(modifiers) => {
                        repaint = true;
                        if !self.state.pending_keybind {
                            self.state.modifiers = modifiers
                        }
                    }
                    WindowEvent::MouseInput { button, state, .. } => {
                        repaint = true;
                        if !self.state.pending_keybind {
                            self.on_input(Input::Mouse(button), state, false);
                        }
                    }
                    WindowEvent::Focused(_)
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::CursorMoved { .. }
                    | WindowEvent::CursorEntered { .. }
                    | WindowEvent::CursorLeft { .. }
                    | WindowEvent::Touch { .. }
                    | WindowEvent::Resized(..)
                    | WindowEvent::Moved(..)
                    | WindowEvent::ThemeChanged(..) => repaint = true,
                    WindowEvent::DroppedFile(path) => {
                        self.nes_event(EmulationEvent::LoadRomPath(path));
                    }
                    WindowEvent::HoveredFile(_) => {
                        // TODO: Show file drop cursor
                        repaint = true;
                    }
                    WindowEvent::HoveredFileCancelled => {
                        repaint = true;
                        // TODO: Restore cursor
                    }
                    _ => (),
                }
            }
            Event::UserEvent(event) => {
                // Only wake emulation of relevant events
                if matches!(event, NesEvent::Emulation(_) | NesEvent::Config(_)) {
                    self.emulation.on_event(&event);
                }
                self.renderer.on_event(&event);
                match event {
                    NesEvent::Config(ConfigEvent::InputBindings) => {
                        self.state.input_bindings =
                            InputBindings::from_input_config(&self.cfg.input);
                    }
                    NesEvent::Renderer(RendererEvent::ScaleChanged) => repaint = true,
                    NesEvent::Ui(event) => match event {
                        UiEvent::Terminate => event_loop.exit(),
                        _ => self.on_event(event),
                    },
                    _ => (),
                }
            }
            Event::LoopExiting => {
                #[cfg(feature = "profiling")]
                puffin::set_scopes_on(false);

                // Save window scale on exit
                let size = self.window.inner_size();
                let scale_factor = self.window.scale_factor() as f32;
                let texture_size = self.cfg.texture_size();
                let scale = if size.width < size.height {
                    (size.width as f32 / scale_factor) / texture_size.width as f32
                } else {
                    (size.height as f32 / scale_factor) / texture_size.height as f32
                };
                self.cfg.renderer.scale = scale.floor();
                if let Err(err) = self.cfg.save() {
                    error!("{err:?}");
                }
            }
            _ => (),
        }
        if repaint {
            self.window.request_redraw();
        }
    }

    pub fn on_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Message(msg) => self.add_message(msg),
            UiEvent::Error(err) => self.on_error(anyhow!(err)),
            UiEvent::IgnoreInputActions(pending) => self.state.pending_keybind = pending,
            UiEvent::LoadRomDialog => {
                match open_file_dialog(
                    "Load ROM",
                    "NES ROMs",
                    &["nes"],
                    self.cfg
                        .renderer
                        .roms_path
                        .as_ref()
                        .map(|p| p.to_path_buf()),
                ) {
                    Ok(maybe_path) => {
                        if let Some(path) = maybe_path {
                            self.nes_event(EmulationEvent::LoadRomPath(path));
                        }
                    }
                    Err(err) => {
                        error!("failed top open rom dialog: {err:?}");
                        self.nes_event(UiEvent::Error("failed to open rom dialog".to_string()));
                    }
                }
            }
            UiEvent::LoadReplayDialog => {
                match open_file_dialog(
                    "Load Replay",
                    "Replay Recording",
                    &["replay"],
                    Config::default_data_dir(),
                ) {
                    Ok(maybe_path) => {
                        if let Some(path) = maybe_path {
                            self.nes_event(EmulationEvent::LoadReplayPath(path));
                        }
                    }
                    Err(err) => {
                        error!("failed top open replay dialog: {err:?}");
                        self.nes_event(UiEvent::Error("failed to open replay dialog".to_string()));
                    }
                }
            }
            UiEvent::Terminate => (), // handled in event_loop
        }
    }

    /// Trigger a custom event.
    pub fn nes_event(&mut self, event: impl Into<NesEvent>) {
        let event = event.into();
        trace!("Nes event: {event:?}");

        self.emulation.on_event(&event);
        self.renderer.on_event(&event);
        match event {
            NesEvent::Ui(event) => self.on_event(event),
            NesEvent::Emulation(EmulationEvent::LoadRomPath(path)) => {
                if let Ok(path) = path.canonicalize() {
                    self.cfg.renderer.recent_roms.insert(path);
                }
            }
            _ => (),
        }
    }

    /// Handle user input mapped to key bindings.
    pub fn on_input(&mut self, input: Input, state: ElementState, repeat: bool) {
        if let Some(action) = self.state.input_bindings.get(&input).copied() {
            trace!("action: {action:?}, state: {state:?}, repeat: {repeat:?}");
            let released = state == ElementState::Released;
            match action {
                Action::Ui(state) if released => match state {
                    Ui::Quit => self.tx.nes_event(UiEvent::Terminate),
                    Ui::TogglePause => {
                        if self.renderer.rom_loaded() {
                            self.state.paused = !self.state.paused;
                            self.nes_event(EmulationEvent::Pause(self.state.paused));
                        }
                    }
                    Ui::LoadRom => {
                        self.state.paused = true;
                        self.nes_event(EmulationEvent::Pause(self.state.paused));
                        self.nes_event(UiEvent::LoadRomDialog);
                    }
                    Ui::UnloadRom => {
                        if self.renderer.rom_loaded() {
                            self.nes_event(EmulationEvent::UnloadRom);
                        }
                    }
                    Ui::LoadReplay => {
                        if self.renderer.rom_loaded() {
                            self.state.paused = true;
                            self.nes_event(EmulationEvent::Pause(self.state.paused));
                            self.nes_event(UiEvent::LoadReplayDialog);
                        }
                    }
                },
                Action::Menu(menu) if released => self.nes_event(RendererEvent::Menu(menu)),
                Action::Feature(feature) => match feature {
                    Feature::ToggleReplayRecording if released => {
                        if platform::supports(platform::Feature::Filesystem) {
                            if self.renderer.rom_loaded() {
                                self.state.replay_recording = !self.state.replay_recording;
                                self.nes_event(EmulationEvent::ReplayRecord(
                                    self.state.replay_recording,
                                ));
                            }
                        } else {
                            self.add_message(
                                "replay recordings are not supported yet on this platform.",
                            );
                        }
                    }
                    Feature::ToggleAudioRecording if released => {
                        if platform::supports(platform::Feature::Filesystem) {
                            if self.renderer.rom_loaded() {
                                self.state.audio_recording = !self.state.audio_recording;
                                self.nes_event(EmulationEvent::AudioRecord(
                                    self.state.audio_recording,
                                ));
                            }
                        } else {
                            self.add_message(
                                "audio recordings are not supported yet on this platform.",
                            );
                        }
                    }
                    Feature::TakeScreenshot if released => {
                        if platform::supports(platform::Feature::Filesystem) {
                            if self.renderer.rom_loaded() {
                                self.nes_event(EmulationEvent::Screenshot);
                            }
                        } else {
                            self.add_message("screenshots are not supported yet on this platform.");
                        }
                    }
                    Feature::VisualRewind => {
                        if !self.state.rewinding {
                            if repeat {
                                self.state.rewinding = true;
                                self.nes_event(EmulationEvent::Rewinding(self.state.rewinding));
                            } else if released {
                                self.nes_event(EmulationEvent::InstantRewind);
                            }
                        } else if released {
                            self.state.rewinding = false;
                            self.nes_event(EmulationEvent::Rewinding(self.state.rewinding));
                        }
                    }
                    _ => (),
                },
                Action::Setting(setting) => match setting {
                    Setting::ToggleFullscreen if released => {
                        self.cfg.renderer.fullscreen = !self.cfg.renderer.fullscreen;
                        self.window.set_fullscreen(
                            self.cfg
                                .renderer
                                .fullscreen
                                .then_some(Fullscreen::Borderless(None)),
                        );
                    }
                    Setting::ToggleVsync if released => {
                        if platform::supports(platform::Feature::ToggleVsync) {
                            self.cfg.renderer.vsync = !self.cfg.renderer.vsync;
                            self.nes_event(ConfigEvent::Vsync(self.cfg.renderer.vsync));
                        } else {
                            self.add_message("Disabling VSync is not supported on this platform.");
                        }
                    }
                    Setting::ToggleAudio if released => {
                        self.cfg.audio.enabled = !self.cfg.audio.enabled;
                        self.nes_event(ConfigEvent::AudioEnabled(self.cfg.audio.enabled));
                    }
                    Setting::ToggleMenubar if released => {
                        self.cfg.renderer.show_menubar = !self.cfg.renderer.show_menubar;
                    }
                    Setting::IncrementScale if released => {
                        let scale = self.cfg.renderer.scale;
                        let new_scale = self.cfg.increment_scale();
                        if scale != new_scale {
                            self.nes_event(RendererEvent::ScaleChanged);
                        }
                    }
                    Setting::DecrementScale if released => {
                        let scale = self.cfg.renderer.scale;
                        let new_scale = self.cfg.decrement_scale();
                        if scale != new_scale {
                            self.nes_event(RendererEvent::ScaleChanged);
                        }
                    }
                    Setting::IncrementSpeed if released => {
                        let speed = self.cfg.emulation.speed;
                        let new_speed = self.cfg.increment_speed();
                        if speed != new_speed {
                            self.nes_event(ConfigEvent::Speed(self.cfg.emulation.speed));
                            self.add_message(format!("Increased Emulation Speed to {new_speed}"));
                        }
                    }
                    Setting::DecrementSpeed if released => {
                        let speed = self.cfg.emulation.speed;
                        let new_speed = self.cfg.decrement_speed();
                        if speed != new_speed {
                            self.nes_event(ConfigEvent::Speed(self.cfg.emulation.speed));
                            self.add_message(format!("Decreased Emulation Speed to {new_speed}"));
                        }
                    }
                    Setting::FastForward if !repeat && self.renderer.rom_loaded() => {
                        let new_speed = if released { 1.0 } else { 2.0 };
                        let speed = self.cfg.emulation.speed;
                        if speed != new_speed {
                            self.cfg.emulation.speed = new_speed;
                            self.nes_event(ConfigEvent::Speed(self.cfg.emulation.speed));
                            if new_speed == 2.0 {
                                self.add_message("Fast forwarding");
                            }
                        }
                    }
                    _ => (),
                },
                Action::Deck(action) => match action {
                    DeckAction::Reset(kind) if released => {
                        self.nes_event(EmulationEvent::Reset(kind));
                    }
                    DeckAction::Joypad((player, button)) if !repeat => {
                        self.nes_event(EmulationEvent::Joypad((player, button, state)));
                    }
                    // Handled by `gui` module
                    DeckAction::ZapperAim(_)
                    | DeckAction::ZapperAimOffscreen
                    | DeckAction::ZapperTrigger => (),
                    DeckAction::SetSaveSlot(slot) if released => {
                        if platform::supports(platform::Feature::Filesystem) {
                            if self.cfg.emulation.save_slot != slot {
                                self.cfg.emulation.save_slot = slot;
                                self.add_message(format!("Changed Save Slot to {slot}"));
                            }
                        } else {
                            self.add_message("save states are not supported yet on this platform.");
                        }
                    }
                    DeckAction::SaveState if released => {
                        if platform::supports(platform::Feature::Filesystem) {
                            self.nes_event(EmulationEvent::SaveState(self.cfg.emulation.save_slot));
                        } else {
                            self.add_message("save states are not supported yet on this platform.");
                        }
                    }
                    DeckAction::LoadState if released => {
                        if platform::supports(platform::Feature::Filesystem) {
                            self.nes_event(EmulationEvent::LoadState(self.cfg.emulation.save_slot));
                        } else {
                            self.add_message("save states are not supported yet on this platform.");
                        }
                    }
                    DeckAction::ToggleApuChannel(channel) if released => {
                        self.cfg.deck.channels_enabled[channel as usize] =
                            !self.cfg.deck.channels_enabled[channel as usize];
                        self.nes_event(ConfigEvent::ApuChannelEnabled((
                            channel,
                            self.cfg.deck.channels_enabled[channel as usize],
                        )));
                    }
                    DeckAction::MapperRevision(rev) if released => {
                        self.cfg.deck.mapper_revisions.set(rev);
                        self.nes_event(ConfigEvent::MapperRevisions(
                            self.cfg.deck.mapper_revisions,
                        ));
                        self.add_message(format!("Changed Mapper Revision to {rev}"));
                    }
                    DeckAction::SetNesRegion(region) if released => {
                        self.cfg.deck.region = region;
                        self.nes_event(ConfigEvent::Region(self.cfg.deck.region));
                        self.add_message(format!("Changed NES Region to {region:?}"));
                    }
                    DeckAction::SetVideoFilter(filter) if released => {
                        let filter = if self.cfg.deck.filter == filter {
                            VideoFilter::Pixellate
                        } else {
                            filter
                        };
                        self.cfg.deck.filter = filter;
                        self.nes_event(ConfigEvent::VideoFilter(filter));
                    }
                    _ => (),
                },
                Action::Debug(action) => match action {
                    Debug::Toggle(kind) if released => {
                        self.add_message(format!("{kind:?} is not implemented yet"));
                    }
                    Debug::Step(step) if released | repeat => {
                        self.nes_event(EmulationEvent::DebugStep(step));
                    }
                    _ => (),
                },
                _ => (),
            }
        }
    }
}
