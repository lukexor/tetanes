use crate::nes::{
    action::{Action, DebugKind, DebugStep, Debugger, Feature, Setting, UiState},
    config::{Config, FrameSpeed, Scale},
    renderer::gui::{ConfigTab, Menu},
    Nes,
};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};
use tetanes_core::{
    action::Action as DeckAction,
    apu::Channel,
    common::{NesRegion, ResetKind},
    control_deck,
    input::{JoypadBtn, Player},
    video::VideoFilter,
};
use tetanes_util::platform::time::Duration;
use tracing::{error, trace};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, Modifiers, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoopWindowTarget},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::Fullscreen,
};

#[derive(Debug, Clone)]
#[must_use]
pub enum UiEvent {
    Error(String),
    Message(String),
    SetTitle(String),
    RequestRedraw,
    ResizeWindow((LogicalSize<f32>, LogicalSize<f32>)),
    LoadRomDialog,
    ConfigChanged(Config),
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
    #[cfg(not(target_arch = "wasm32"))]
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
                    #[cfg(not(target_arch = "wasm32"))]
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
                    error!("failed to save config: {err:?}");
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
            UiEvent::SetTitle(title) => self.window.set_title(&title),
            UiEvent::ResizeWindow((inner_size, min_inner_size)) => {
                let _ = self.window.request_inner_size(inner_size);
                self.window.set_min_inner_size(Some(min_inner_size));
            }
            UiEvent::RequestRedraw => self.window.request_redraw(),
            UiEvent::ConfigChanged(ref config) => {
                self.config = config.clone();
                self.emulation
                    .on_event(&Event::UserEvent(event.clone().into()));
                self.renderer
                    .on_event(&self.window, &Event::UserEvent(event.clone().into()));
            }
            #[cfg(target_arch = "wasm32")]
            UiEvent::LoadRomDialog => {
                use crate::nes::platform::html_ids;
                use wasm_bindgen::JsCast;
                use web_sys::HtmlInputElement;

                let input = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.get_element_by_id(html_ids::ROM_INPUT))
                    .and_then(|input| input.dyn_into::<HtmlInputElement>().ok());
                match input {
                    Some(input) => input.click(),
                    None => self.trigger_event(UiEvent::Error("failed to open rom".to_string())),
                }
                if let Some(canvas) = crate::nes::platform::get_canvas() {
                    let _ = canvas.focus();
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            UiEvent::LoadRomDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("NES ROMs", &["nes"])
                    .pick_file()
                {
                    self.trigger_event(EmulationEvent::LoadRomPath((path, self.config.clone())));
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
            NesEvent::Emulation(_) => self.emulation.on_event(&Event::UserEvent(event)),
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

macro_rules! key_map {
    ($map:expr, $player:expr, $key:expr, $action:expr) => {
        $map.insert(
            Input::Key($key, ModifiersState::empty()),
            ($player, $action.into()),
        );
    };
    ($map:expr, $player:expr, $key:expr, $modifiers:expr, $action:expr) => {
        $map.insert(Input::Key($key, $modifiers), ($player, $action.into()));
    };
}

macro_rules! mouse_map {
    ($map:expr, $player:expr, $button:expr, $action:expr) => {
        $map.insert(
            Input::Mouse($button, ElementState::Released),
            ($player, $action.into()),
        );
    };
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Input {
    Key(KeyCode, ModifiersState),
    Mouse(MouseButton, ElementState),
}

pub type InputBinding = (Input, Player, Action);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputMap(HashMap<Input, (Player, Action)>);

impl InputMap {
    pub fn from_bindings(bindings: &[InputBinding]) -> Self {
        let mut map = HashMap::with_capacity(bindings.len());
        for (input, player, action) in bindings {
            map.insert(*input, (*player, *action));
        }
        map.shrink_to_fit();
        Self(map)
    }
}

impl Deref for InputMap {
    type Target = HashMap<Input, (Player, Action)>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for InputMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for InputMap {
    fn default() -> Self {
        use KeyCode::*;
        use Player::*;
        const SHIFT: ModifiersState = ModifiersState::SHIFT;
        const CONTROL: ModifiersState = ModifiersState::CONTROL;

        let mut map = HashMap::new();

        key_map!(map, One, ArrowLeft, JoypadBtn::Left);
        key_map!(map, One, ArrowRight, JoypadBtn::Right);
        key_map!(map, One, ArrowUp, JoypadBtn::Up);
        key_map!(map, One, ArrowDown, JoypadBtn::Down);
        key_map!(map, One, KeyZ, JoypadBtn::A);
        key_map!(map, One, KeyX, JoypadBtn::B);
        key_map!(map, One, KeyA, JoypadBtn::TurboA);
        key_map!(map, One, KeyS, JoypadBtn::TurboB);
        key_map!(map, One, Enter, JoypadBtn::Start);
        key_map!(map, One, ShiftRight, JoypadBtn::Select);
        key_map!(map, One, ShiftLeft, JoypadBtn::Select);
        key_map!(map, One, ShiftRight, SHIFT, JoypadBtn::Select); // Required because shift is also a modifier
        key_map!(map, One, ShiftLeft, SHIFT, JoypadBtn::Select); // Required because shift is also a modifier
        key_map!(map, Two, KeyJ, JoypadBtn::Left);
        key_map!(map, Two, KeyL, JoypadBtn::Right);
        key_map!(map, Two, KeyI, JoypadBtn::Up);
        key_map!(map, Two, KeyK, JoypadBtn::Down);
        key_map!(map, Two, KeyN, JoypadBtn::A);
        key_map!(map, Two, KeyM, JoypadBtn::B);
        key_map!(map, Two, Numpad8, JoypadBtn::Start);
        key_map!(map, Two, Numpad9, SHIFT, JoypadBtn::Select);
        key_map!(map, Three, KeyF, JoypadBtn::Left);
        key_map!(map, Three, KeyH, JoypadBtn::Right);
        key_map!(map, Three, KeyT, JoypadBtn::Up);
        key_map!(map, Three, KeyG, JoypadBtn::Down);
        key_map!(map, Three, KeyV, JoypadBtn::A);
        key_map!(map, Three, KeyB, JoypadBtn::B);
        key_map!(map, Three, Numpad5, JoypadBtn::Start);
        key_map!(map, Three, Numpad6, SHIFT, JoypadBtn::Select);
        key_map!(map, One, Escape, UiState::TogglePause);
        key_map!(map, One, KeyH, CONTROL, Menu::About);
        key_map!(map, One, F1, Menu::About);
        key_map!(map, One, KeyC, CONTROL, Menu::Config(ConfigTab::General));
        key_map!(map, One, F2, Menu::Config(ConfigTab::General));
        key_map!(map, One, KeyO, CONTROL, Menu::LoadRom);
        key_map!(map, One, F3, Menu::LoadRom);
        key_map!(map, One, KeyK, CONTROL, Menu::Keybind(Player::One));
        key_map!(map, One, KeyQ, CONTROL, UiState::Quit);
        key_map!(map, One, KeyR, CONTROL, DeckAction::SoftReset);
        key_map!(map, One, KeyP, CONTROL, DeckAction::HardReset);
        key_map!(map, One, Equal, CONTROL, Setting::IncSpeed);
        key_map!(map, One, Minus, CONTROL, Setting::DecSpeed);
        key_map!(map, One, Space, Setting::FastForward);
        key_map!(map, One, Digit1, CONTROL, DeckAction::SetSaveSlot(1));
        key_map!(map, One, Digit2, CONTROL, DeckAction::SetSaveSlot(2));
        key_map!(map, One, Digit3, CONTROL, DeckAction::SetSaveSlot(3));
        key_map!(map, One, Digit4, CONTROL, DeckAction::SetSaveSlot(4));
        key_map!(map, One, Numpad1, CONTROL, DeckAction::SetSaveSlot(1));
        key_map!(map, One, Numpad2, CONTROL, DeckAction::SetSaveSlot(2));
        key_map!(map, One, Numpad3, CONTROL, DeckAction::SetSaveSlot(3));
        key_map!(map, One, Numpad4, CONTROL, DeckAction::SetSaveSlot(4));
        key_map!(map, One, KeyS, CONTROL, DeckAction::SaveState);
        key_map!(map, One, KeyL, CONTROL, DeckAction::LoadState);
        key_map!(map, One, KeyR, Feature::Rewind);
        key_map!(map, One, F10, Feature::TakeScreenshot);
        key_map!(map, One, KeyV, SHIFT, Feature::ToggleReplayRecord);
        key_map!(map, One, KeyR, SHIFT, Feature::ToggleAudioRecord);
        key_map!(map, One, KeyM, CONTROL, Setting::ToggleAudio);
        key_map!(
            map,
            One,
            Digit1,
            SHIFT,
            DeckAction::ToggleApuChannel(Channel::Pulse1)
        );
        key_map!(
            map,
            One,
            Digit2,
            SHIFT,
            DeckAction::ToggleApuChannel(Channel::Pulse2)
        );
        key_map!(
            map,
            One,
            Digit3,
            SHIFT,
            DeckAction::ToggleApuChannel(Channel::Triangle)
        );
        key_map!(
            map,
            One,
            Digit4,
            SHIFT,
            DeckAction::ToggleApuChannel(Channel::Noise)
        );
        key_map!(
            map,
            One,
            Digit5,
            SHIFT,
            DeckAction::ToggleApuChannel(Channel::Dmc)
        );
        key_map!(map, One, Enter, CONTROL, Setting::ToggleFullscreen);
        key_map!(map, One, KeyV, CONTROL, Setting::ToggleVsync);
        key_map!(
            map,
            One,
            KeyN,
            CONTROL,
            DeckAction::SetVideoFilter(VideoFilter::Ntsc)
        );
        key_map!(
            map,
            One,
            KeyD,
            SHIFT,
            Debugger::ToggleDebugger(DebugKind::Cpu)
        );
        key_map!(
            map,
            One,
            KeyP,
            SHIFT,
            Debugger::ToggleDebugger(DebugKind::Ppu)
        );
        key_map!(
            map,
            One,
            KeyA,
            SHIFT,
            Debugger::ToggleDebugger(DebugKind::Apu)
        );
        key_map!(map, One, KeyC, Debugger::Step(DebugStep::Into));
        key_map!(map, One, KeyO, Debugger::Step(DebugStep::Over));
        key_map!(map, One, KeyO, SHIFT, Debugger::Step(DebugStep::Out));
        key_map!(map, One, KeyL, SHIFT, Debugger::Step(DebugStep::Scanline));
        key_map!(map, One, KeyF, SHIFT, Debugger::Step(DebugStep::Frame));
        key_map!(map, One, ArrowDown, CONTROL, Debugger::UpdateScanline(1));
        key_map!(map, One, ArrowUp, CONTROL, Debugger::UpdateScanline(-1));
        key_map!(
            map,
            One,
            ArrowDown,
            SHIFT | CONTROL,
            Debugger::UpdateScanline(10)
        );
        key_map!(
            map,
            One,
            ArrowUp,
            SHIFT | CONTROL,
            Debugger::UpdateScanline(-10)
        );

        mouse_map!(map, Two, MouseButton::Left, DeckAction::ZapperTrigger);

        Self(map)
    }
}
