use crate::{
    feature,
    nes::{
        action::{Action, Debug, DebugKind, DebugStep, Feature, Setting, Ui},
        config::Config,
        emulation::FrameStats,
        input::{ActionBindings, AxisDirection, Gamepads, Input, InputBindings},
        renderer::{
            gui::{Menu, MessageType},
            shader::Shader,
        },
        rom::RomData,
        Nes, RunState, Running, State,
    },
    platform::open_file_dialog,
};
use anyhow::anyhow;
use egui::ViewportId;
use std::path::PathBuf;
use tetanes_core::{
    action::Action as DeckAction,
    apu::{Apu, Channel},
    common::{NesRegion, ResetKind},
    control_deck::{LoadedRom, MapperRevisionsConfig},
    debug::Debugger,
    genie::GenieCode,
    input::{FourPlayer, JoypadBtn, Player},
    mem::RamState,
    ppu::Ppu,
    time::{Duration, Instant},
    video::VideoFilter,
};
use tracing::{debug, error, trace};
use uuid::Uuid;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop, EventLoopProxy},
    keyboard::PhysicalKey,
    window::WindowId,
};

#[derive(Default, Debug, Copy, Clone)]
#[must_use]
pub struct Response {
    pub consumed: bool,
    pub repaint: bool,
}

#[derive(Debug, Clone)]
pub struct NesEventProxy(EventLoopProxy<NesEvent>);

impl NesEventProxy {
    pub fn new(event_loop: &EventLoop<NesEvent>) -> Self {
        Self(event_loop.create_proxy())
    }

    pub fn event(&self, event: impl Into<NesEvent>) {
        let event = event.into();
        trace!("sending event: {event:?}");
        if let Err(err) = self.0.send_event(event) {
            error!("failed to send event: {err:?}");
            std::process::exit(1);
        }
    }

    pub const fn inner(&self) -> &EventLoopProxy<NesEvent> {
        &self.0
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub enum NesEvent {
    // For some reason accesskit_winit::Event isn't Clone
    #[cfg(not(target_arch = "wasm32"))]
    AccessKit {
        window_id: WindowId,
        event: AccessKitWindowEvent,
    },
    Config(ConfigEvent),
    Debug(DebugEvent),
    Emulation(EmulationEvent),
    Renderer(RendererEvent),
    Ui(UiEvent),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub enum AccessKitWindowEvent {
    InitialTreeRequested,
    ActionRequested(accesskit::ActionRequest),
    AccessibilityDeactivated,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<accesskit_winit::Event> for NesEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        use accesskit_winit::WindowEvent;
        Self::AccessKit {
            window_id: event.window_id,
            event: match event.window_event {
                WindowEvent::InitialTreeRequested => AccessKitWindowEvent::InitialTreeRequested,
                WindowEvent::ActionRequested(request) => {
                    AccessKitWindowEvent::ActionRequested(request)
                }
                WindowEvent::AccessibilityDeactivated => {
                    AccessKitWindowEvent::AccessibilityDeactivated
                }
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum ConfigEvent {
    ActionBindings(Vec<ActionBindings>),
    ActionBindingSet((Action, Input, usize)),
    ActionBindingClear(Input),
    AlwaysOnTop(bool),
    ApuChannelEnabled((Channel, bool)),
    ApuChannelsEnabled([bool; Apu::MAX_CHANNEL_COUNT]),
    AudioBuffer(usize),
    AudioEnabled(bool),
    AudioLatency(Duration),
    AutoLoad(bool),
    AutoSave(bool),
    AutoSaveInterval(Duration),
    ConcurrentDpad(bool),
    CycleAccurate(bool),
    DarkTheme(bool),
    EmbedViewports(bool),
    FourPlayer(FourPlayer),
    Fullscreen(bool),
    GamepadAssign((Player, Uuid)),
    GamepadAssignments([(Player, Option<Uuid>); 4]),
    GamepadUnassign(Player),
    GenieCodeAdded(GenieCode),
    GenieCodeClear,
    GenieCodeRemoved(String),
    HideOverscan(bool),
    MapperRevisions(MapperRevisionsConfig),
    RamState(RamState),
    RecentRomsClear,
    Region(NesRegion),
    RewindEnabled(bool),
    RewindInterval(u32),
    RewindSeconds(u32),
    RunAhead(usize),
    SaveSlot(u8),
    Scale(f32),
    Shader(Shader),
    ShowMenubar(bool),
    ShowMessages(bool),
    Speed(f32),
    VideoFilter(VideoFilter),
    ZapperConnected(bool),
}

impl From<ConfigEvent> for NesEvent {
    fn from(event: ConfigEvent) -> Self {
        Self::Config(event)
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub enum DebugEvent {
    Ppu(Ppu),
}

impl From<DebugEvent> for NesEvent {
    fn from(event: DebugEvent) -> Self {
        Self::Debug(event)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum EmulationEvent {
    AddDebugger(Debugger),
    RemoveDebugger(Debugger),
    AudioRecord(bool),
    DebugStep(DebugStep),
    EmulatePpuWarmup(bool),
    InstantRewind,
    Joypad((Player, JoypadBtn, ElementState)),
    LoadReplay((String, ReplayData)),
    LoadReplayPath(PathBuf),
    LoadRom((String, RomData)),
    LoadRomPath(PathBuf),
    LoadState(u8),
    RunState(RunState),
    ReplayRecord(bool),
    Reset(ResetKind),
    RequestFrame,
    Rewinding(bool),
    SaveState(u8),
    ShowFrameStats(bool),
    Screenshot,
    UnloadRom,
    ZapperAim((u32, u32)),
    ZapperTrigger,
}

impl From<EmulationEvent> for NesEvent {
    fn from(event: EmulationEvent) -> Self {
        Self::Emulation(event)
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub enum RendererEvent {
    ViewportResized((f32, f32)),
    FrameStats(FrameStats),
    ShowMenubar(bool),
    ToggleFullscreen,
    ReplayLoaded,
    ResizeTexture,
    ResizeWindow,
    ResourcesReady,
    RequestRedraw {
        viewport_id: ViewportId,
        when: Instant,
    },
    RomLoaded(LoadedRom),
    RomUnloaded,
    Menu(Menu),
}

impl From<RendererEvent> for NesEvent {
    fn from(event: RendererEvent) -> Self {
        Self::Renderer(event)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum UiEvent {
    Error(String),
    Message((MessageType, String)),
    UpdateAvailable(String),
    LoadRomDialog,
    LoadReplayDialog,
    FileDialogCancelled,
    Terminate,
}

impl From<UiEvent> for NesEvent {
    fn from(event: UiEvent) -> Self {
        Self::Ui(event)
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

impl ApplicationHandler<NesEvent> for Nes {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: NesEvent) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        trace!("user event: {event:?}");

        match event {
            NesEvent::Renderer(RendererEvent::ResourcesReady) => {
                if let Err(err) = self.init_running() {
                    error!("failed to create window: {err:?}");
                    event_loop.exit();
                    return;
                }

                // Disable device events to save some cpu as they're mostly duplicated in
                // WindowEvents
                event_loop.listen_device_events(DeviceEvents::Never);

                if let State::Running(state) = &mut self.state {
                    if let Some(window) = state.renderer.root_window() {
                        if window.is_visible().unwrap_or(true) {
                            state.repaint_times.insert(window.id(), Instant::now());
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
            NesEvent::Ui(UiEvent::Terminate) => event_loop.exit(),
            _ => (),
        }

        if let State::Running(state) = &mut self.state {
            state.user_event(event_loop, event);
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        debug!("resumed event");

        let state = if let State::Running(state) = &mut self.state {
            if feature!(Suspend) {
                state.renderer.recreate_window(event_loop);
            }
            state
        } else {
            if self.state.is_suspended() {
                if let Err(err) = self.request_renderer_resources(event_loop) {
                    error!("failed to request renderer resources: {err:?}");
                    event_loop.exit();
                }
            }
            return;
        };
        if let Some(window_id) = state.renderer.root_window_id() {
            state.repaint_times.insert(window_id, Instant::now());
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        trace!("window event: {window_id:?} {event:?}");

        if let State::Running(state) = &mut self.state {
            state.window_event(event_loop, window_id, event);
        }
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        trace!("device event: {device_id:?} {event:?}");

        if let State::Running(state) = &mut self.state {
            state.device_event(event_loop, device_id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if let State::Running(state) = &mut self.state {
            state.about_to_wait(event_loop);
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        debug!("suspended event");

        if let State::Running(state) = &mut self.state {
            state.suspended(event_loop);
        }
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        debug!("exiting");

        #[cfg(feature = "profiling")]
        puffin::set_scopes_on(false);

        if let State::Running(state) = &mut self.state {
            state.exiting(event_loop);
        } else if feature!(AbortOnExit) {
            panic!("exited unexpectedly");
        }
    }
}

impl ApplicationHandler<NesEvent> for Running {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: NesEvent) {
        match event {
            NesEvent::Config(ref event) => {
                let Config {
                    deck,
                    emulation,
                    audio,
                    renderer,
                    input,
                } = &mut self.cfg;
                match event {
                    ConfigEvent::ActionBindings(bindings) => {
                        input.action_bindings.clone_from(bindings);
                        self.input_bindings = InputBindings::from_input_config(input);
                    }
                    ConfigEvent::ActionBindingSet((action, set_input, binding)) => {
                        input.set_binding(*action, *set_input, *binding);
                        self.input_bindings.insert(*set_input, *action);
                    }
                    ConfigEvent::ActionBindingClear(clear_input) => {
                        input.clear_binding(*clear_input);
                        self.input_bindings.remove(clear_input);
                    }
                    ConfigEvent::AlwaysOnTop(always_on_top) => {
                        renderer.always_on_top = *always_on_top;
                        self.renderer
                            .set_always_on_top(self.cfg.renderer.always_on_top);
                    }
                    ConfigEvent::ApuChannelEnabled((channel, enabled)) => {
                        deck.channels_enabled[*channel as usize] = *enabled;
                    }
                    ConfigEvent::ApuChannelsEnabled(enabled) => {
                        deck.channels_enabled = *enabled;
                    }
                    ConfigEvent::AudioBuffer(buffer_size) => {
                        audio.buffer_size = *buffer_size;
                    }
                    ConfigEvent::AudioEnabled(enabled) => audio.enabled = *enabled,
                    ConfigEvent::AudioLatency(latency) => audio.latency = *latency,
                    ConfigEvent::AutoLoad(enabled) => emulation.auto_load = *enabled,
                    ConfigEvent::AutoSave(enabled) => emulation.auto_save = *enabled,
                    ConfigEvent::AutoSaveInterval(interval) => {
                        emulation.auto_save_interval = *interval;
                    }
                    ConfigEvent::ConcurrentDpad(enabled) => deck.concurrent_dpad = *enabled,
                    ConfigEvent::CycleAccurate(enabled) => deck.cycle_accurate = *enabled,
                    ConfigEvent::DarkTheme(enabled) => renderer.dark_theme = *enabled,
                    ConfigEvent::EmbedViewports(embed) => renderer.embed_viewports = *embed,
                    ConfigEvent::FourPlayer(four_player) => deck.four_player = *four_player,
                    ConfigEvent::Fullscreen(fullscreen) => renderer.fullscreen = *fullscreen,
                    ConfigEvent::GamepadAssign((player, uuid)) => {
                        input.assign_gamepad(*player, *uuid);
                        if let Some(name) = self.gamepads.gamepad_name_by_uuid(uuid) {
                            self.tx.event(UiEvent::Message((
                                MessageType::Info,
                                format!("Assigned gamepad `{name}` to player {player:?}.",),
                            )));
                        }
                    }
                    ConfigEvent::GamepadUnassign(player) => {
                        if let Some(uuid) = input.unassign_gamepad(*player) {
                            if let Some(name) = self.gamepads.gamepad_name_by_uuid(&uuid) {
                                self.tx.event(UiEvent::Message((
                                    MessageType::Info,
                                    format!("Unassigned gamepad `{name}` from player {player:?}."),
                                )));
                            }
                        }
                    }
                    ConfigEvent::GamepadAssignments(assignments) => {
                        input.gamepad_assignments = *assignments;
                    }
                    ConfigEvent::GenieCodeAdded(genie_code) => {
                        deck.genie_codes.push(genie_code.clone());
                    }
                    ConfigEvent::GenieCodeClear => deck.genie_codes.clear(),
                    ConfigEvent::GenieCodeRemoved(code) => {
                        deck.genie_codes.retain(|genie| genie.code() != code);
                    }
                    ConfigEvent::HideOverscan(hide) => renderer.hide_overscan = *hide,
                    ConfigEvent::MapperRevisions(revs) => deck.mapper_revisions = *revs,
                    ConfigEvent::RamState(ram_state) => deck.ram_state = *ram_state,
                    ConfigEvent::RecentRomsClear => renderer.recent_roms.clear(),
                    ConfigEvent::Region(region) => deck.region = *region,
                    ConfigEvent::RewindEnabled(enabled) => emulation.rewind = *enabled,
                    ConfigEvent::RewindInterval(interval) => {
                        emulation.rewind_interval = *interval;
                    }
                    ConfigEvent::RewindSeconds(seconds) => {
                        emulation.rewind_seconds = *seconds;
                    }
                    ConfigEvent::RunAhead(run_ahead) => emulation.run_ahead = *run_ahead,
                    ConfigEvent::SaveSlot(slot) => emulation.save_slot = *slot,
                    ConfigEvent::Scale(scale) => renderer.scale = *scale,
                    ConfigEvent::Shader(shader) => renderer.shader = *shader,
                    ConfigEvent::ShowMenubar(show) => renderer.show_menubar = *show,
                    ConfigEvent::ShowMessages(show) => renderer.show_messages = *show,
                    ConfigEvent::Speed(speed) => emulation.speed = *speed,
                    ConfigEvent::VideoFilter(filter) => deck.filter = *filter,
                    ConfigEvent::ZapperConnected(connected) => deck.zapper = *connected,
                }

                self.renderer.prepare(&self.gamepads, &self.cfg);
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
            NesEvent::Ui(ref event) => self.on_ui_event(event),
            _ => (),
        }

        // Only wake emulation of relevant events
        if matches!(event, NesEvent::Emulation(_) | NesEvent::Config(_)) {
            self.emulation.on_event(&event);
        }
        self.renderer.on_event(&mut event, &self.cfg);
    }

    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let res = self.renderer.on_window_event(window_id, &event);
        if res.repaint && event != WindowEvent::RedrawRequested {
            self.repaint_times.insert(window_id, Instant::now());
        }

        if !res.consumed {
            match event {
                WindowEvent::RedrawRequested => {
                    self.emulation.try_clock_frame();

                    if let Err(err) = self.renderer.redraw(
                        window_id,
                        event_loop,
                        &mut self.gamepads,
                        &mut self.cfg,
                    ) {
                        self.renderer.on_error(err);
                    }
                    self.repaint_times.remove(&window_id);
                }
                WindowEvent::Resized(_) => {
                    if Some(window_id) == self.renderer.root_window_id() {
                        self.cfg.renderer.fullscreen = self.renderer.fullscreen();
                    }
                }
                WindowEvent::Focused(focused) => {
                    if focused {
                        self.repaint_times.insert(window_id, Instant::now());
                        if self.renderer.rom_loaded() && self.run_state.auto_paused() {
                            self.run_state = RunState::Running;
                            self.event(EmulationEvent::RunState(self.run_state));
                        }
                    } else {
                        let time_since_last_save = Instant::now() - self.renderer.last_save_time;
                        if time_since_last_save > Duration::from_secs(30) {
                            if let Err(err) = self.renderer.save(&self.cfg) {
                                error!("failed to save rendererer state: {err:?}");
                            }
                        }
                        if self
                            .renderer
                            .window(window_id)
                            .and_then(|win| win.is_minimized())
                            .unwrap_or(false)
                        {
                            self.repaint_times.remove(&window_id);
                            if self.renderer.rom_loaded() {
                                self.run_state = RunState::Paused;
                                self.event(EmulationEvent::RunState(self.run_state));
                            }
                        }
                    }
                }
                WindowEvent::Occluded(occluded) => {
                    // Note: Does not trigger on all platforms (e.g. linux)
                    if occluded {
                        self.repaint_times.remove(&window_id);
                        if self.renderer.rom_loaded() {
                            self.run_state = RunState::Paused;
                            self.event(EmulationEvent::RunState(self.run_state));
                        }
                    } else {
                        self.repaint_times.insert(window_id, Instant::now());
                        if self.renderer.rom_loaded() && self.run_state.auto_paused() {
                            self.run_state = RunState::Running;
                            self.event(EmulationEvent::RunState(self.run_state));
                        }
                    }
                }
                WindowEvent::KeyboardInput {
                    event,
                    is_synthetic,
                    ..
                } => {
                    // Winit generates fake "synthetic" KeyboardInput events when the focus
                    // is changed to the window, or away from it. Synthetic key presses
                    // represent no real key presses and should be ignored.
                    // See https://github.com/rust-windowing/winit/issues/3543
                    if !is_synthetic || event.state != ElementState::Pressed {
                        if let PhysicalKey::Code(key) = event.physical_key {
                            self.on_input(
                                window_id,
                                Input::Key(key, self.modifiers.state()),
                                event.state,
                                event.repeat,
                            );
                        }
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
                        self.event(EmulationEvent::LoadRomPath(path));
                    }
                }
                _ => (),
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.renderer.on_mouse_motion(delta);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        self.gamepads.update_events();
        if let Some(window_id) = self.renderer.root_window_id() {
            let res = self.renderer.on_gamepad_update(&self.gamepads);
            if res.repaint {
                self.repaint_times.insert(window_id, Instant::now());
            }

            if res.consumed {
                self.gamepads.clear_events();
            } else {
                while let Some(event) = self.gamepads.next_event() {
                    self.on_gamepad_event(window_id, event);
                    self.repaint_times.insert(window_id, Instant::now());
                }
            }
        }

        self.update_repaint_times(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if feature!(Suspend) {
            if let Err(err) = self.renderer.drop_window() {
                error!("failed to suspend window: {err:?}");
                event_loop.exit();
            }
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Err(err) = self.renderer.save(&self.cfg) {
            error!("failed to save rendererer state: {err:?}");
        }
        self.renderer.destroy();

        if feature!(AbortOnExit) {
            panic!("exited unexpectedly");
        }
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        self.renderer
            .add_message(MessageType::Warn, "Your system memory is running low...");
        if self.cfg.emulation.rewind {
            self.cfg.emulation.rewind = false;
            self.event(ConfigEvent::RewindEnabled(false));
        }
    }
}

impl Running {
    pub fn update_repaint_times(&mut self, event_loop: &ActiveEventLoop) {
        let mut next_repaint_time = self.repaint_times.values().min().copied();
        self.repaint_times.retain(|window_id, when| {
            if *when > Instant::now() {
                return true;
            }
            next_repaint_time = None;

            if let Some(window) = self.renderer.window(*window_id) {
                if !window.is_minimized().unwrap_or(false) {
                    window.request_redraw();
                }
                // Repaint time will get removed as soon as we receive the RequestRedraw event
                true
            } else {
                false
            }
        });

        event_loop.set_control_flow(ControlFlow::WaitUntil(match next_repaint_time {
            Some(next_repaint_time) => next_repaint_time,
            None => Instant::now() + Duration::from_millis(16),
        }));
    }

    pub fn on_ui_event(&mut self, event: &UiEvent) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        match event {
            UiEvent::Message((ty, msg)) => self.renderer.add_message(*ty, msg),
            UiEvent::Error(err) => self.renderer.on_error(anyhow!(err.clone())),
            UiEvent::LoadRomDialog => {
                match open_file_dialog(
                    "Load ROM",
                    "NES ROMs",
                    &["nes"],
                    self.cfg.renderer.roms_path.as_ref(),
                ) {
                    Ok(maybe_path) => {
                        if let Some(path) = maybe_path {
                            self.event(EmulationEvent::LoadRomPath(path));
                        }
                    }
                    Err(err) => {
                        error!("failed top open rom dialog: {err:?}");
                        self.event(UiEvent::Error("failed to open rom dialog".to_string()));
                    }
                }
            }
            UiEvent::LoadReplayDialog => {
                match open_file_dialog(
                    "Load Replay",
                    "Replay Recording",
                    &["replay"],
                    Some(Config::default_data_dir()),
                ) {
                    Ok(maybe_path) => {
                        if let Some(path) = maybe_path {
                            self.event(EmulationEvent::LoadReplayPath(path));
                        }
                    }
                    Err(err) => {
                        error!("failed top open replay dialog: {err:?}");
                        self.event(UiEvent::Error("failed to open replay dialog".to_string()));
                    }
                }
            }
            UiEvent::FileDialogCancelled => {
                if self.renderer.rom_loaded() {
                    self.run_state = RunState::Running;
                    self.event(EmulationEvent::RunState(self.run_state));
                }
            }
            UiEvent::UpdateAvailable(_) | UiEvent::Terminate => (),
        }
    }

    /// Trigger a custom event.
    pub fn event(&mut self, event: impl Into<NesEvent>) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut event = event.into();
        trace!("Nes event: {event:?}");

        self.emulation.on_event(&event);
        self.renderer.on_event(&mut event, &self.cfg);
        match event {
            NesEvent::Ui(event) => self.on_ui_event(&event),
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

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

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

        self.renderer.prepare(&self.gamepads, &self.cfg);
    }

    /// Handle user input mapped to key bindings.
    pub fn on_input(
        &mut self,
        window_id: WindowId,
        input: Input,
        state: ElementState,
        repeat: bool,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if let Some(action) = self.input_bindings.get(&input).copied() {
            trace!("action: {action:?}, state: {state:?}, repeat: {repeat:?}");
            let released = state == ElementState::Released;
            let is_root_window = Some(window_id) == self.renderer.root_window_id();
            match action {
                Action::Ui(ui_state) if released => match ui_state {
                    Ui::Quit => self.tx.event(UiEvent::Terminate),
                    Ui::TogglePause => {
                        if is_root_window && self.renderer.rom_loaded() {
                            self.run_state = match self.run_state {
                                RunState::Running => RunState::ManuallyPaused,
                                RunState::ManuallyPaused | RunState::Paused => RunState::Running,
                            };
                            self.event(EmulationEvent::RunState(self.run_state));
                        }
                    }
                    Ui::LoadRom => {
                        if self.renderer.rom_loaded() {
                            self.run_state = RunState::Paused;
                            self.event(EmulationEvent::RunState(self.run_state));
                        }
                        // NOTE: Due to some platforms file dialogs blocking the event loop,
                        // loading requires a round-trip in order for the above pause to
                        // get processed.
                        self.tx.event(UiEvent::LoadRomDialog);
                    }
                    Ui::UnloadRom => {
                        if self.renderer.rom_loaded() {
                            self.event(EmulationEvent::UnloadRom);
                        }
                    }
                    Ui::LoadReplay => {
                        if self.renderer.rom_loaded() {
                            self.run_state = RunState::Paused;
                            self.event(EmulationEvent::RunState(self.run_state));
                            // NOTE: Due to some platforms file dialogs blocking the event loop,
                            // loading requires a round-trip in order for the above pause to
                            // get processed.
                            self.tx.event(UiEvent::LoadReplayDialog);
                        }
                    }
                },
                Action::Menu(menu) if released => self.event(RendererEvent::Menu(menu)),
                Action::Feature(feature) if is_root_window => match feature {
                    Feature::ToggleReplayRecording if released => {
                        if feature!(Filesystem) {
                            if self.renderer.rom_loaded() {
                                self.replay_recording = !self.replay_recording;
                                self.event(EmulationEvent::ReplayRecord(self.replay_recording));
                            }
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                "Replay recordings are not supported yet on this platform.",
                            );
                        }
                    }
                    Feature::ToggleAudioRecording if released => {
                        if feature!(Filesystem) {
                            if self.renderer.rom_loaded() {
                                self.audio_recording = !self.audio_recording;
                                self.event(EmulationEvent::AudioRecord(self.audio_recording));
                            }
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                "Audio recordings are not supported yet on this platform.",
                            );
                        }
                    }
                    Feature::TakeScreenshot if released => {
                        if feature!(Filesystem) {
                            if self.renderer.rom_loaded() {
                                self.event(EmulationEvent::Screenshot);
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
                                self.event(EmulationEvent::Rewinding(self.rewinding));
                            } else if released {
                                self.event(EmulationEvent::InstantRewind);
                            }
                        } else if released {
                            self.rewinding = false;
                            self.event(EmulationEvent::Rewinding(self.rewinding));
                        }
                    }
                    _ => (),
                },
                Action::Setting(setting) => match setting {
                    Setting::ToggleFullscreen if released => {
                        self.cfg.renderer.fullscreen = !self.cfg.renderer.fullscreen;
                        self.renderer.set_fullscreen(
                            self.cfg.renderer.fullscreen,
                            self.cfg.renderer.embed_viewports,
                        );
                    }
                    Setting::ToggleEmbedViewports if released => {
                        self.cfg.renderer.embed_viewports = !self.cfg.renderer.embed_viewports;
                        self.renderer
                            .set_embed_viewports(self.cfg.renderer.embed_viewports);
                    }
                    Setting::ToggleAlwaysOnTop if released => {
                        self.cfg.renderer.always_on_top = !self.cfg.renderer.always_on_top;
                        self.renderer
                            .set_always_on_top(self.cfg.renderer.always_on_top);
                    }
                    Setting::ToggleAudio if released => {
                        self.cfg.audio.enabled = !self.cfg.audio.enabled;
                        self.event(ConfigEvent::AudioEnabled(self.cfg.audio.enabled));
                    }
                    Setting::ToggleMenubar if released => {
                        self.cfg.renderer.show_menubar = !self.cfg.renderer.show_menubar;
                        self.event(RendererEvent::ShowMenubar(self.cfg.renderer.show_menubar));
                    }
                    Setting::IncrementScale if released => {
                        let scale = self.cfg.renderer.scale;
                        let new_scale = self.cfg.increment_scale();
                        if scale != new_scale {
                            self.event(ConfigEvent::Scale(new_scale));
                        }
                    }
                    Setting::DecrementScale if released => {
                        let scale = self.cfg.renderer.scale;
                        let new_scale = self.cfg.decrement_scale();
                        if scale != new_scale {
                            self.event(ConfigEvent::Scale(new_scale));
                        }
                    }
                    Setting::IncrementSpeed if released => {
                        let speed = self.cfg.emulation.speed;
                        let new_speed = self.cfg.increment_speed();
                        if speed != new_speed {
                            self.event(ConfigEvent::Speed(self.cfg.emulation.speed));
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
                            self.event(ConfigEvent::Speed(self.cfg.emulation.speed));
                            self.renderer.add_message(
                                MessageType::Info,
                                format!("Decreased Emulation Speed to {new_speed}"),
                            );
                        }
                    }
                    Setting::FastForward
                        if !repeat && is_root_window && self.renderer.rom_loaded() =>
                    {
                        let new_speed = if released { 1.0 } else { 2.0 };
                        let speed = self.cfg.emulation.speed;
                        if speed != new_speed {
                            self.cfg.emulation.speed = new_speed;
                            self.event(ConfigEvent::Speed(self.cfg.emulation.speed));
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
                        self.event(EmulationEvent::Reset(kind));
                        self.run_state = RunState::Running;
                        self.event(EmulationEvent::RunState(self.run_state));
                    }
                    DeckAction::Joypad((player, button)) if !repeat && is_root_window => {
                        self.event(EmulationEvent::Joypad((player, button, state)));
                    }
                    // Handled by `gui` module
                    DeckAction::ZapperAim(_)
                    | DeckAction::ZapperAimOffscreen
                    | DeckAction::ZapperTrigger => (),
                    DeckAction::SetSaveSlot(slot) if released => {
                        if feature!(Storage) {
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
                    DeckAction::SaveState if released && is_root_window => {
                        if feature!(Storage) {
                            self.event(EmulationEvent::SaveState(self.cfg.emulation.save_slot));
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                "Save states are not supported yet on this platform.",
                            );
                        }
                    }
                    DeckAction::LoadState if released && is_root_window => {
                        if feature!(Storage) {
                            self.event(EmulationEvent::LoadState(self.cfg.emulation.save_slot));
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
                        self.event(ConfigEvent::ApuChannelEnabled((
                            channel,
                            self.cfg.deck.channels_enabled[channel as usize],
                        )));
                    }
                    DeckAction::MapperRevision(rev) if released => {
                        self.cfg.deck.mapper_revisions.set(rev);
                        self.event(ConfigEvent::MapperRevisions(self.cfg.deck.mapper_revisions));
                        self.renderer.add_message(
                            MessageType::Info,
                            format!("Changed Mapper Revision to {rev}"),
                        );
                    }
                    DeckAction::SetNesRegion(region) if released => {
                        self.cfg.deck.region = region;
                        self.event(ConfigEvent::Region(self.cfg.deck.region));
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
                        self.event(ConfigEvent::VideoFilter(filter));
                    }
                    _ => (),
                },
                Action::Debug(action) => match action {
                    Debug::Toggle(kind) if released => {
                        if matches!(kind, DebugKind::Ppu) {
                            self.event(RendererEvent::Menu(Menu::PpuViewer));
                        } else {
                            self.renderer.add_message(
                                MessageType::Warn,
                                format!("{kind:?} is not implemented yet"),
                            );
                        }
                    }
                    Debug::Step(step) if (released | repeat) && is_root_window => {
                        self.event(EmulationEvent::DebugStep(step));
                    }
                    _ => (),
                },
                _ => (),
            }
        }
    }
}
