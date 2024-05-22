use crate::{
    nes::{
        action::{Action, Debug, DebugStep, Feature, Setting, Ui},
        config::Config,
        emulation::FrameStats,
        input::{AxisDirection, Gamepads, Input, InputBindings},
        renderer::gui::{Menu, MessageType},
        rom::RomData,
        Nes, Running, State,
    },
    platform::{self, open_file_dialog},
};
use anyhow::anyhow;
use egui::ViewportId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tetanes_core::{
    action::Action as DeckAction,
    apu::Channel,
    common::{NesRegion, ResetKind},
    control_deck::{LoadedRom, MapperRevisionsConfig},
    genie::GenieCode,
    input::{FourPlayer, JoypadBtn, Player},
    mem::RamState,
    time::{Duration, Instant},
    video::VideoFilter,
};
use tracing::{error, trace};
use winit::{
    event::{ElementState, Event, WindowEvent},
    event_loop::{ControlFlow, DeviceEvents, EventLoopProxy, EventLoopWindowTarget},
    keyboard::PhysicalKey,
    window::WindowId,
};

pub trait SendNesEvent {
    fn nes_event(&self, event: impl Into<NesEvent>);
}

impl SendNesEvent for EventLoopProxy<NesEvent> {
    fn nes_event(&self, event: impl Into<NesEvent>) {
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
    Message((MessageType, String)),
    LoadRomDialog,
    LoadReplayDialog,
    FileDialogCancelled,
    Terminate,
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
    AutoSaveInterval(Duration),
    ConcurrentDpad(bool),
    CycleAccurate(bool),
    FourPlayer(FourPlayer),
    GenieCodeAdded(GenieCode),
    GenieCodeRemoved(String),
    HideOverscan(bool),
    InputBindings,
    MapperRevisions(MapperRevisionsConfig),
    RamState(RamState),
    Region(NesRegion),
    RewindEnabled(bool),
    RewindSeconds(u32),
    RewindInterval(u32),
    RunAhead(usize),
    SaveSlot(u8),
    Speed(f32),
    VideoFilter(VideoFilter),
    ZapperConnected(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum EmulationEvent {
    AudioRecord(bool),
    DebugStep(DebugStep),
    EmulatePpuWarmup(bool),
    InstantRewind,
    Joypad((Player, JoypadBtn, ElementState)),
    #[serde(skip)]
    LoadReplay((String, ReplayData)),
    LoadReplayPath(PathBuf),
    #[serde(skip)]
    LoadRom((String, RomData)),
    LoadRomPath(PathBuf),
    LoadState(u8),
    UnfocusedPause(bool),
    Pause(bool),
    ReplayRecord(bool),
    Reset(ResetKind),
    Rewinding(bool),
    SaveState(u8),
    ShowFrameStats(bool),
    Screenshot,
    UnloadRom,
    ZapperAim((u32, u32)),
    ZapperTrigger,
}

#[derive(Debug, Clone)]
#[must_use]
pub enum RendererEvent {
    #[cfg(target_arch = "wasm32")]
    BrowserResized((f32, f32)),
    FrameStats(FrameStats),
    ShowMenubar(bool),
    ScaleChanged,
    ResourcesReady,
    RequestRedraw {
        viewport_id: ViewportId,
        when: Instant,
    },
    RomLoaded(LoadedRom),
    RomUnloaded,
    Menu(Menu),
}

#[derive(Debug, Clone)]
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

impl Nes {
    pub fn event_loop(
        &mut self,
        event: Event<NesEvent>,
        event_loop: &EventLoopWindowTarget<NesEvent>,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if !matches!(event, Event::NewEvents(..) | Event::AboutToWait) {
            trace!("event: {:?}", event);
        }

        match event {
            Event::Resumed => {
                let state = if let State::Running(state) = &mut self.state {
                    if platform::supports(platform::Feature::Suspend) {
                        state.renderer.recreate_window(event_loop);
                    }
                    state
                } else {
                    if self.state.is_suspended() {
                        if let Err(err) = self.request_resources(event_loop) {
                            error!("failed to request renderer resources: {err:?}");
                            event_loop.exit();
                        }
                    }
                    return;
                };
                state.repaint_times.insert(
                    state
                        .renderer
                        .root_window_id()
                        .expect("failed to get root window_id"),
                    Instant::now(),
                );
            }
            Event::UserEvent(NesEvent::Renderer(RendererEvent::ResourcesReady)) => {
                if let Err(err) = self.init_running(event_loop) {
                    error!("failed to create window: {err:?}");
                    event_loop.exit();
                    return;
                }
                // Disable device events to save some cpu as they're mostly duplicated in
                // WindowEvents
                event_loop.listen_device_events(DeviceEvents::Never);
                if let State::Running(state) = &mut self.state {
                    if let Some(window) = state
                        .renderer
                        .root_window_id()
                        .and_then(|id| state.renderer.window(id))
                    {
                        if window.is_visible().unwrap_or(true) {
                            state.repaint_times.insert(
                                state
                                    .renderer
                                    .root_window_id()
                                    .expect("failed to get root window_id"),
                                Instant::now(),
                            );
                        } else {
                            // Immediately redraw the root window on start if not
                            // visible. Fixes a bug where `window.request_redraw()` events
                            // may not be sent if the window isn't visible, which is the
                            // case until the first frame is drawn.
                            if let Err(err) = state.renderer.redraw(
                                window.id(),
                                event_loop,
                                &mut state.gamepads,
                                &mut state.cfg,
                            ) {
                                state.renderer.on_error(err);
                            }
                        }
                    }
                }
            }
            _ => (),
        }

        if let State::Running(state) = &mut self.state {
            state.on_event(event, event_loop);

            let mut next_repaint_time = state.repaint_times.values().min().copied();
            state.repaint_times.retain(|window_id, when| {
                if Instant::now() < *when {
                    return true;
                }
                next_repaint_time = None;
                event_loop.set_control_flow(ControlFlow::Poll);

                if let Some(window) = state.renderer.window(*window_id) {
                    if !window.is_minimized().unwrap_or(false) {
                        window.request_redraw();
                    }
                    true
                } else {
                    false
                }
            });

            if let Some(next_repaint_time) = next_repaint_time {
                event_loop.set_control_flow(ControlFlow::WaitUntil(next_repaint_time));
            }
        }
    }
}

impl Running {
    pub fn on_event(
        &mut self,
        event: Event<NesEvent>,
        event_loop: &EventLoopWindowTarget<NesEvent>,
    ) {
        match event {
            Event::Suspended => {
                if platform::supports(platform::Feature::Suspend) {
                    if let Err(err) = self.renderer.drop_window() {
                        error!("failed to suspend window: {err:?}");
                        event_loop.exit();
                    }
                }
            }
            Event::MemoryWarning => {
                self.renderer
                    .add_message(MessageType::Warn, "Your system memory is running low...");
                if self.cfg.emulation.rewind {
                    self.nes_event(ConfigEvent::RewindEnabled(false));
                }
            }
            Event::AboutToWait => {
                self.gamepads.update_events();
                if let Some(window_id) = self.renderer.root_window_id() {
                    let res = self.renderer.on_gamepad_update(&self.gamepads);
                    if res.repaint {
                        self.repaint_times.insert(window_id, Instant::now());
                    }

                    if !res.consumed {
                        while let Some(event) = self.gamepads.next_event() {
                            self.on_gamepad_event(window_id, event);
                            self.repaint_times.insert(window_id, Instant::now());
                        }
                    }
                }

                self.emulation.clock_frame();
            }
            Event::WindowEvent {
                window_id, event, ..
            } => {
                let res = self.renderer.on_window_event(
                    window_id,
                    &event,
                    #[cfg(target_arch = "wasm32")]
                    &self.cfg,
                );
                if res.repaint {
                    self.repaint_times.insert(window_id, Instant::now());
                }

                if !res.consumed {
                    match event {
                        WindowEvent::RedrawRequested => {
                            self.repaint_times.remove(&window_id);
                            if let Err(err) = self.renderer.redraw(
                                window_id,
                                event_loop,
                                &mut self.gamepads,
                                &mut self.cfg,
                            ) {
                                self.renderer.on_error(err);
                            }
                        }
                        WindowEvent::Resized(_) => {
                            if Some(window_id) == self.renderer.root_window_id() {
                                self.cfg.renderer.fullscreen = self.renderer.fullscreen();
                            }
                        }
                        WindowEvent::Focused(focused) => {
                            if focused {
                                self.repaint_times.insert(window_id, Instant::now());
                            }
                        }
                        WindowEvent::Occluded(occluded) => {
                            // Note: Does not trigger on all platforms
                            if !occluded {
                                self.repaint_times.insert(window_id, Instant::now());
                            }
                        }
                        WindowEvent::KeyboardInput { event, .. } => {
                            if let PhysicalKey::Code(key) = event.physical_key {
                                self.on_input(
                                    window_id,
                                    Input::Key(key, self.modifiers.state()),
                                    event.state,
                                    event.repeat,
                                );
                            }
                        }
                        WindowEvent::ModifiersChanged(modifiers) => {
                            self.modifiers = modifiers;
                        }
                        WindowEvent::MouseInput { button, state, .. } => {
                            self.on_input(window_id, Input::Mouse(button), state, false);
                        }
                        WindowEvent::DroppedFile(path) => {
                            if Some(window_id) == self.renderer.root_window_id() {
                                self.nes_event(EmulationEvent::LoadRomPath(path));
                            }
                        }
                        _ => (),
                    }
                }
            }
            Event::UserEvent(event) => {
                // Only wake emulation of relevant events
                if matches!(event, NesEvent::Emulation(_) | NesEvent::Config(_)) {
                    self.emulation.on_event(&event);
                }
                self.renderer.on_event(
                    &event,
                    #[cfg(target_arch = "wasm32")]
                    &self.cfg,
                );

                match event {
                    NesEvent::Config(ConfigEvent::InputBindings) => {
                        self.input_bindings = InputBindings::from_input_config(&self.cfg.input);
                    }
                    NesEvent::Renderer(RendererEvent::RequestRedraw { viewport_id, when }) => {
                        if let Some(window_id) = self.renderer.window_id_for_viewport(viewport_id) {
                            self.repaint_times.insert(
                                window_id,
                                self.repaint_times
                                    .get(&window_id)
                                    .map_or(when, |last| (*last).min(when)),
                            );
                        }
                    }
                    NesEvent::Ui(event) => {
                        if let UiEvent::Terminate = event {
                            event_loop.exit()
                        } else {
                            self.on_ui_event(event);
                        }
                    }
                    _ => (),
                }
            }
            Event::LoopExiting => {
                #[cfg(feature = "profiling")]
                puffin::set_scopes_on(false);

                self.renderer.destroy();

                if let Err(err) = self.cfg.save() {
                    error!("failed to save configuration: {err:?}");
                }
            }
            _ => (),
        }
    }

    pub fn on_ui_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Message((ty, msg)) => self.renderer.add_message(ty, msg),
            UiEvent::Error(err) => self.renderer.on_error(anyhow!(err)),
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
            UiEvent::FileDialogCancelled => {
                if self.renderer.rom_loaded() {
                    self.paused = false;
                    self.nes_event(EmulationEvent::Pause(self.paused));
                }
            }
            UiEvent::Terminate => (),
        }
    }

    /// Trigger a custom event.
    pub fn nes_event(&mut self, event: impl Into<NesEvent>) {
        let event = event.into();
        trace!("Nes event: {event:?}");

        self.emulation.on_event(&event);
        self.renderer.on_event(
            &event,
            #[cfg(target_arch = "wasm32")]
            &self.cfg,
        );
        match event {
            NesEvent::Ui(event) => self.on_ui_event(event),
            NesEvent::Emulation(EmulationEvent::LoadRomPath(path)) => {
                if let Ok(path) = path.canonicalize() {
                    self.cfg.renderer.recent_roms.insert(path);
                }
            }
            _ => (),
        }
    }

    /// Handle gamepad event.
    pub fn on_gamepad_event(&mut self, window_id: WindowId, event: gilrs::Event) {
        use gilrs::EventType;

        // Connect first because we may not have a name set yet
        if event.event == EventType::Connected {
            self.gamepads.connect(event.id);
        }

        if let Some(uuid) = self.gamepads.gamepad_uuid(event.id) {
            match event.event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(player) = self.cfg.input.gamepad_assignment(&uuid) {
                        self.on_input(
                            window_id,
                            Input::Button(player, button),
                            ElementState::Pressed,
                            false,
                        );
                    }
                }
                EventType::ButtonRepeated(button, _) => {
                    if let Some(player) = self.cfg.input.gamepad_assignment(&uuid) {
                        self.on_input(
                            window_id,
                            Input::Button(player, button),
                            ElementState::Pressed,
                            true,
                        );
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if let Some(player) = self.cfg.input.gamepad_assignment(&uuid) {
                        self.on_input(
                            window_id,
                            Input::Button(player, button),
                            ElementState::Released,
                            false,
                        );
                    }
                }
                EventType::AxisChanged(axis, value, _) => {
                    if let Some(player) = self.cfg.input.gamepad_assignment(&uuid) {
                        if let (Some(direction), state) = Gamepads::axis_state(value) {
                            self.on_input(
                                window_id,
                                Input::Axis(player, axis, direction),
                                state,
                                false,
                            );
                        } else {
                            for direction in [AxisDirection::Positive, AxisDirection::Negative] {
                                self.on_input(
                                    window_id,
                                    Input::Axis(player, axis, direction),
                                    ElementState::Released,
                                    false,
                                );
                            }
                        }
                    }
                }
                EventType::Connected => {
                    let saved_assignment = self.cfg.input.gamepad_assignment(&uuid);
                    if let Some(player) =
                        saved_assignment.or_else(|| self.cfg.input.next_gamepad_unassigned())
                    {
                        if let Some(name) = self.gamepads.gamepad_name_by_uuid(&uuid) {
                            self.renderer.add_message(
                                MessageType::Info,
                                format!("Assigned gamepad `{name}` to player {player:?}."),
                            );
                            self.cfg.input.assign_gamepad(player, uuid);
                        }
                    }
                }
                EventType::Disconnected => {
                    self.gamepads.disconnect(event.id);
                    if let Some(player) = self.cfg.input.unassign_gamepad_name(&uuid) {
                        if let Some(name) = self.gamepads.gamepad_name_by_uuid(&uuid) {
                            self.renderer.add_message(
                                MessageType::Info,
                                format!("Unassigned gamepad `{name}` from player {player:?}."),
                            );
                        }
                    }
                }
                _ => (),
            }
        }
    }

    /// Handle user input mapped to key bindings.
    pub fn on_input(
        &mut self,
        window_id: WindowId,
        input: Input,
        state: ElementState,
        repeat: bool,
    ) {
        if let Some(action) = self.input_bindings.get(&input).copied() {
            trace!("action: {action:?}, state: {state:?}, repeat: {repeat:?}");
            let released = state == ElementState::Released;
            let root_window = Some(window_id) == self.renderer.root_window_id();
            match action {
                Action::Ui(ui_state) if released => match ui_state {
                    Ui::Quit => self.tx.nes_event(UiEvent::Terminate),
                    Ui::TogglePause => {
                        if root_window && self.renderer.rom_loaded() {
                            self.paused = !self.paused;
                            self.nes_event(EmulationEvent::Pause(self.paused));
                        }
                    }
                    Ui::LoadRom => {
                        if self.renderer.rom_loaded() {
                            self.paused = true;
                            self.nes_event(EmulationEvent::Pause(self.paused));
                        }
                        // NOTE: Due to some platforms file dialogs blocking the event loop,
                        // loading requires a round-trip in order for the above pause to
                        // get processed.
                        self.tx.nes_event(UiEvent::LoadRomDialog);
                    }
                    Ui::UnloadRom => {
                        if self.renderer.rom_loaded() {
                            self.nes_event(EmulationEvent::UnloadRom);
                        }
                    }
                    Ui::LoadReplay => {
                        if self.renderer.rom_loaded() {
                            self.paused = true;
                            self.nes_event(EmulationEvent::Pause(self.paused));
                            // NOTE: Due to some platforms file dialogs blocking the event loop,
                            // loading requires a round-trip in order for the above pause to
                            // get processed.
                            self.tx.nes_event(UiEvent::LoadReplayDialog);
                        }
                    }
                },
                Action::Menu(menu) if released => self.nes_event(RendererEvent::Menu(menu)),
                Action::Feature(feature) if root_window => match feature {
                    Feature::ToggleReplayRecording if released => {
                        if platform::supports(platform::Feature::Filesystem) {
                            if self.renderer.rom_loaded() {
                                self.replay_recording = !self.replay_recording;
                                self.nes_event(EmulationEvent::ReplayRecord(self.replay_recording));
                            }
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                "Replay recordings are not supported yet on this platform.",
                            );
                        }
                    }
                    Feature::ToggleAudioRecording if released => {
                        if platform::supports(platform::Feature::Filesystem) {
                            if self.renderer.rom_loaded() {
                                self.audio_recording = !self.audio_recording;
                                self.nes_event(EmulationEvent::AudioRecord(self.audio_recording));
                            }
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                "Audio recordings are not supported yet on this platform.",
                            );
                        }
                    }
                    Feature::TakeScreenshot if released => {
                        if platform::supports(platform::Feature::Filesystem) {
                            if self.renderer.rom_loaded() {
                                self.nes_event(EmulationEvent::Screenshot);
                            }
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                "Screenshots are not supported yet on this platform.",
                            );
                        }
                    }
                    Feature::VisualRewind => {
                        if !self.rewinding {
                            if repeat {
                                self.rewinding = true;
                                self.nes_event(EmulationEvent::Rewinding(self.rewinding));
                            } else if released {
                                self.nes_event(EmulationEvent::InstantRewind);
                            }
                        } else if released {
                            self.rewinding = false;
                            self.nes_event(EmulationEvent::Rewinding(self.rewinding));
                        }
                    }
                    _ => (),
                },
                Action::Setting(setting) => match setting {
                    Setting::ToggleFullscreen if released && root_window => {
                        self.cfg.renderer.fullscreen = !self.cfg.renderer.fullscreen;
                        self.renderer.set_fullscreen(self.cfg.renderer.fullscreen);
                    }
                    Setting::ToggleAudio if released => {
                        self.cfg.audio.enabled = !self.cfg.audio.enabled;
                        self.nes_event(ConfigEvent::AudioEnabled(self.cfg.audio.enabled));
                    }
                    Setting::ToggleMenubar if released => {
                        self.cfg.renderer.show_menubar = !self.cfg.renderer.show_menubar;
                        self.nes_event(RendererEvent::ShowMenubar(self.cfg.renderer.show_menubar));
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
                            self.renderer.add_message(
                                MessageType::Info,
                                format!("Increased Emulation Speed to {new_speed}"),
                            );
                        }
                    }
                    Setting::DecrementSpeed if released => {
                        let speed = self.cfg.emulation.speed;
                        let new_speed = self.cfg.decrement_speed();
                        if speed != new_speed {
                            self.nes_event(ConfigEvent::Speed(self.cfg.emulation.speed));
                            self.renderer.add_message(
                                MessageType::Info,
                                format!("Decreased Emulation Speed to {new_speed}"),
                            );
                        }
                    }
                    Setting::FastForward
                        if !repeat && root_window && self.renderer.rom_loaded() =>
                    {
                        let new_speed = if released { 1.0 } else { 2.0 };
                        let speed = self.cfg.emulation.speed;
                        if speed != new_speed {
                            self.cfg.emulation.speed = new_speed;
                            self.nes_event(ConfigEvent::Speed(self.cfg.emulation.speed));
                            if new_speed == 2.0 {
                                self.renderer
                                    .add_message(MessageType::Info, "Fast forwarding");
                            }
                        }
                    }
                    _ => (),
                },
                Action::Deck(action) => match action {
                    DeckAction::Reset(kind) if released => {
                        self.nes_event(EmulationEvent::Reset(kind));
                    }
                    DeckAction::Joypad((player, button)) if !repeat && root_window => {
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
                                self.renderer.add_message(
                                    MessageType::Info,
                                    format!("Changed Save Slot to {slot}"),
                                );
                            }
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                "Save states are not supported yet on this platform.",
                            );
                        }
                    }
                    DeckAction::SaveState if released && root_window => {
                        if platform::supports(platform::Feature::Filesystem) {
                            self.nes_event(EmulationEvent::SaveState(self.cfg.emulation.save_slot));
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                "Save states are not supported yet on this platform.",
                            );
                        }
                    }
                    DeckAction::LoadState if released && root_window => {
                        if platform::supports(platform::Feature::Filesystem) {
                            self.nes_event(EmulationEvent::LoadState(self.cfg.emulation.save_slot));
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                "Save states are not supported yet on this platform.",
                            );
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
                        self.renderer.add_message(
                            MessageType::Info,
                            format!("Changed Mapper Revision to {rev}"),
                        );
                    }
                    DeckAction::SetNesRegion(region) if released => {
                        self.cfg.deck.region = region;
                        self.nes_event(ConfigEvent::Region(self.cfg.deck.region));
                        self.renderer.add_message(
                            MessageType::Info,
                            format!("Changed NES Region to {region:?}"),
                        );
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
                        self.renderer.add_message(
                            MessageType::Warn,
                            format!("{kind:?} is not implemented yet"),
                        );
                    }
                    Debug::Step(step) if (released | repeat) && root_window => {
                        self.nes_event(EmulationEvent::DebugStep(step));
                    }
                    _ => (),
                },
                _ => (),
            }
        }
    }
}
