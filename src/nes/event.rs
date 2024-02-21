use crate::{
    apu::Channel,
    common::{NesRegion, ResetKind},
    input::{JoypadBtn, Player},
    mapper::MapperRevision,
    nes::{
        config::{Config, Scale, Speed},
        gui::{ConfigTab, Menu},
        Nes,
    },
    profile,
    video::VideoFilter,
};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};
use winit::{
    event::{ElementState, Event as WinitEvent, Modifiers, MouseButton, WindowEvent},
    event_loop::EventLoopWindowTarget,
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::Fullscreen,
};

#[derive(Clone)]
#[must_use]
pub enum NesEvent {
    Message(String),
    Error(String),
    ConfigUpdate(Config),
    Terminate,
}

impl std::fmt::Debug for NesEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(msg) => write!(f, "NesEvent::Message({msg:?})"),
            Self::Error(err) => write!(f, "NesEvent::Error({err:?})"),
            Self::ConfigUpdate(_) => write!(f, "NesEvent::ConfigUpdate(..)"),
            Self::Terminate => write!(f, "NesEvent::Terminate"),
        }
    }
}

impl From<NesEvent> for WinitEvent<Event> {
    fn from(event: NesEvent) -> Self {
        Self::UserEvent(event.into())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub enum DeckEvent {
    LoadRom((String, Vec<u8>)),
    Joypad((Player, JoypadBtn, ElementState)),
    TriggerZapper,
    Occluded(bool),
    Pause(bool),
    TogglePause,
    Reset(ResetKind),
    ToggleReplayRecord,
    ToggleAudioRecord,
    ToggleAudio,
    ToggleApuChannel(Channel),
    ToggleVideoFilter(VideoFilter),
    Screenshot,
    SaveState,
    LoadState,
    SetSaveSlot(u8),
    SetFrameSpeed(Speed),
    Rewind((ElementState, bool)),
}

impl std::fmt::Debug for DeckEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoadRom((name, _)) => write!(f, "DeckEvent::LoadRom({name:?}, ..)"),
            Self::Joypad(joypad) => write!(f, "DeckEvent::Joypad({joypad:?})"),
            Self::TriggerZapper => write!(f, "DeckEvent::TriggerZapper"),
            Self::Occluded(occluded) => write!(f, "DeckEvent::Occluded({occluded:?})"),
            Self::Pause(paused) => write!(f, "DeckEvent::Pause({paused:?})"),
            Self::TogglePause => write!(f, "DeckEvent::TogglePause"),
            Self::Reset(kind) => write!(f, "DeckEvent::Reset({kind:?})"),
            Self::ToggleReplayRecord => write!(f, "DeckEvent::ToggleReplayRecord"),
            Self::ToggleAudioRecord => write!(f, "DeckEvent::ToggleAudioRecord"),
            Self::ToggleAudio => write!(f, "DeckEvent::ToggleAudio"),
            Self::ToggleApuChannel(channel) => {
                write!(f, "DeckEvent::ToggleApuChannel({channel:?})")
            }
            Self::ToggleVideoFilter(filter) => {
                write!(f, "DeckEvent::ToggleVideoFilter({filter:?})")
            }
            Self::Screenshot => write!(f, "DeckEvent::Screenshot"),
            Self::SaveState => write!(f, "DeckEvent::SaveState"),
            Self::LoadState => write!(f, "DeckEvent::LoadState"),
            Self::SetSaveSlot(slot) => write!(f, "DeckEvent::SetSaveSlot({slot:?})"),
            Self::SetFrameSpeed(speed) => write!(f, "DeckEvent::SetFrameSpeed({speed:?})"),
            Self::Rewind((state, repeat)) => {
                write!(f, "DeckEvent::Rewind({state:?}, {repeat:?})")
            }
        }
    }
}

impl From<DeckEvent> for WinitEvent<Event> {
    fn from(event: DeckEvent) -> Self {
        Self::UserEvent(event.into())
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub enum RendererEvent {
    SetVSync(bool),
    SetScale(Scale),
}

impl From<RendererEvent> for WinitEvent<Event> {
    fn from(event: RendererEvent) -> Self {
        Self::UserEvent(event.into())
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub enum Event {
    Nes(NesEvent),
    ControlDeck(DeckEvent),
    Renderer(RendererEvent),
    // TODO: Verify if DeviceEvent is sufficient or if manual handling is needed
    //     ControllerAxisMotion {
    //         device_id: DeviceId,
    //         axis: AxisId,
    //         value: f64,
    //     },
    //     ControllerInput {
    //         device_id: DeviceId,
    //         button: ControllerButton,
    //         state: ElementState,
    //     },
    //     ControllerUpdate {
    //         device_id: DeviceId,
    //         update: ControllerUpdate,
    //     },
}

impl From<NesEvent> for Event {
    fn from(event: NesEvent) -> Self {
        Self::Nes(event)
    }
}

impl From<DeckEvent> for Event {
    fn from(event: DeckEvent) -> Self {
        Self::ControlDeck(event)
    }
}

impl From<RendererEvent> for Event {
    fn from(event: RendererEvent) -> Self {
        Self::Renderer(event)
    }
}

impl From<Event> for WinitEvent<Event> {
    fn from(event: Event) -> Self {
        Self::UserEvent(event)
    }
}

#[derive(Default, Debug)]
#[must_use]
pub struct State {
    pub paused: bool,
    pub occluded: bool,
    pub modifiers: Modifiers,
    pub quitting: bool,
}

impl Nes {
    pub fn on_event(
        &mut self,
        event: WinitEvent<Event>,
        window_target: &EventLoopWindowTarget<Event>,
    ) {
        profile!();

        if self.event_state.quitting {
            window_target.exit();
        }

        if let Err(err) = self.emulation.on_event(&event) {
            self.on_error(err);
        }
        if let Err(err) = self.renderer.on_event(&event) {
            self.on_error(err);
        }

        match event {
            WinitEvent::WindowEvent {
                window_id, event, ..
            } => match event {
                WindowEvent::CloseRequested => {
                    if window_id == self.window.id() {
                        window_target.exit();
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Err(err) = self
                        .renderer
                        .request_redraw(self.event_state.paused, &mut self.config)
                    {
                        self.on_error(err);
                    }
                }
                WindowEvent::Occluded(occluded) => {
                    if window_id == self.window.id() {
                        self.send_event(DeckEvent::Occluded(occluded));
                    }
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(key) = event.physical_key {
                        self.on_input(
                            Input::Key(key, self.event_state.modifiers.state()),
                            event.state,
                            event.repeat,
                        );
                    }
                }
                WindowEvent::ModifiersChanged(modifiers) => self.event_state.modifiers = modifiers,
                WindowEvent::MouseInput { button, state, .. } => {
                    self.on_input(Input::Mouse(button, state), state, false)
                }
                WindowEvent::HoveredFile(_) => (), // TODO: Show file drop cursor
                WindowEvent::HoveredFileCancelled => (), // TODO: Restore cursor
                _ => (),
            },
            WinitEvent::AboutToWait => self.next_frame(window_target),
            WinitEvent::UserEvent(Event::Nes(event)) => match event {
                NesEvent::Message(msg) => self.add_message(msg),
                NesEvent::Error(err) => self.on_error(anyhow!(err)),
                NesEvent::ConfigUpdate(config) => self.config = config,
                NesEvent::Terminate => self.quit(),
            },
            WinitEvent::LoopExiting => {
                #[cfg(feature = "profiling")]
                crate::profiling::enable(false);
                if let Err(err) = self.config.save() {
                    log::error!("failed to save config: {err:?}");
                }
            }
            // WinitEvent::DeviceEvent { device_id, event } => todo!(),
            // TODO: Controller support
            // Event::UserEvent(event) => match event {
            //     CustomEvent::ControllerAxisMotion {
            //         device_id,
            //         axis,
            //         value,
            //         ..
            //     } => {
            //         self.handle_controller_axis_motion(device_id, axis, value);
            //     }
            //     CustomEvent::ControllerInput {
            //         device_id,
            //         button,
            //         state,
            //         ..
            //     } => {
            //         self.handle_controller_event(device_id, button, state);
            //     }
            //     CustomEvent::ControllerUpdate {
            //         device_id, update, ..
            //     } => {
            //         self.handle_controller_update(device_id, button, state);
            //     }
            // },
            _ => (),
        }
    }

    /// Send a custom event to the event loop.
    pub fn send_event(&mut self, event: impl Into<Event>) {
        let event = event.into();
        log::debug!("Nes event: {event:?}");
        if let Err(err) = self.event_proxy.send_event(event) {
            log::error!("failed to send nes event: {err:?}");
            std::process::exit(1);
        }
    }

    pub fn pause(&mut self, paused: bool) {
        self.event_state.paused = paused;
        self.send_event(DeckEvent::Pause(paused));
    }

    /// Handle user input mapped to key bindings.
    pub fn on_input(&mut self, input: Input, state: ElementState, repeat: bool) {
        if let Some((player, action)) = self.config.input_map.get(&input).copied() {
            log::trace!(
                "player: {player:?}, action: {action:?}, state: {state:?}, repeat: {repeat:?}"
            );
            let released = state == ElementState::Released;
            match action {
                Action::Nes(nes_state) => self.on_state_action(nes_state, state),
                Action::Menu(menu) if released => self.renderer.toggle_menu(menu),
                Action::Feature(feature) => self.on_feature_action(feature, state, repeat),
                Action::Setting(setting) => self.on_setting_action(setting, state, repeat),
                Action::Joypad(button) if !repeat => {
                    self.send_event(DeckEvent::Joypad((player, button, state)));
                }
                Action::ZapperTrigger if self.config.control_deck.zapper => {
                    self.send_event(DeckEvent::TriggerZapper);
                }
                Action::Debug(action) => self.on_debug_action(action, state, repeat),
                _ => (),
            }
        }
    }

    fn on_state_action(&mut self, nes_state: NesState, state: ElementState) {
        if state != ElementState::Released {
            return;
        }
        match nes_state {
            NesState::Quit => self.event_state.quitting = true,
            NesState::TogglePause => self.send_event(DeckEvent::TogglePause),
            NesState::SoftReset => self.send_event(DeckEvent::Reset(ResetKind::Soft)),
            NesState::HardReset => self.send_event(DeckEvent::Reset(ResetKind::Hard)),
            NesState::MapperRevision(_) => todo!("mapper revision"),
        }
    }

    fn on_feature_action(&mut self, feature: Feature, state: ElementState, repeat: bool) {
        let released = state == ElementState::Released;
        match feature {
            Feature::ToggleReplayRecord if released => {
                self.send_event(DeckEvent::ToggleReplayRecord);
            }
            Feature::ToggleAudioRecord if released => {
                self.send_event(DeckEvent::ToggleAudioRecord);
            }
            Feature::TakeScreenshot if released => self.send_event(DeckEvent::Screenshot),
            Feature::SaveState if released => self.send_event(DeckEvent::SaveState),
            Feature::LoadState if released => self.send_event(DeckEvent::LoadState),
            Feature::Rewind => self.send_event(DeckEvent::Rewind((state, repeat))),
            _ => (),
        }
    }

    fn on_setting_action(&mut self, setting: Setting, state: ElementState, repeat: bool) {
        let released = state != ElementState::Pressed;
        match setting {
            Setting::SetSaveSlot(slot) if released => self.send_event(DeckEvent::SetSaveSlot(slot)),
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
                self.send_event(RendererEvent::SetVSync(self.config.vsync));
            }
            Setting::ToggleVideoFilter(filter) if released => {
                self.send_event(DeckEvent::ToggleVideoFilter(filter));
            }
            Setting::ToggleAudio if released => self.send_event(DeckEvent::ToggleAudio),
            Setting::ToggleApuChannel(channel) if released => {
                self.send_event(DeckEvent::ToggleApuChannel(channel));
            }
            Setting::IncSpeed if released => self.set_speed(self.config.frame_speed.increment()),
            Setting::DecSpeed if released => self.set_speed(self.config.frame_speed.decrement()),
            Setting::FastForward if !repeat => self.set_speed(if released {
                Speed::default()
            } else {
                Speed::X200
            }),
            _ => (),
        }
    }

    fn on_debug_action(&mut self, action: DebugAction, state: ElementState, _repeat: bool) {
        if state != ElementState::Released {
            return;
        }
        match action {
            // DebugAction::ToggleCpuDebugger if !repeat => self.toggle_debugger()?,
            // DebugAction::TogglePpuDebugger if !repeat => self.toggle_ppu_viewer()?,
            // DebugAction::ToggleApuDebugger if !repeat => self.toggle_apu_viewer()?,
            // DebugAction::StepInto if debugging => self.debug_step_into()?,
            // DebugAction::StepOver if debugging => self.debug_step_over()?,
            // DebugAction::StepOut if debugging => self.debug_step_out()?,
            // DebugAction::StepFrame if debugging => self.debug_step_frame()?,
            // DebugAction::StepScanline if debugging => self.debug_step_scanline()?,
            DebugAction::IncScanline => {
                // TODO: add ppu viewer
                // if let Some(ref mut viewer) = self.ppu_viewer {
                // TODO: check keydown
                // let increment = if s.keymod_down(ModifiersState::SHIFT) { 10 } else { 1 };
                // viewer.inc_scanline(increment);
            }
            DebugAction::DecScanline => {
                // TODO: add ppu viewer
                // if let Some(ref mut viewer) = self.ppu_viewer {
                // TODO: check keydown
                // let decrement = if s.keymod_down(ModifiersState::SHIFT) { 10 } else { 1 };
                // viewer.dec_scanline(decrement);
            }
            _ => (),
        }
    }

    /// Quit the application.
    pub fn quit(&mut self) {
        self.event_state.quitting = true;
    }
}

// #[derive(Debug, Copy, Clone, PartialEq)]
// #[must_use]
// pub struct DeviceId(usize);

// #[derive(Debug, Copy, Clone, PartialEq, Eq)]
// #[must_use]
// pub enum ControllerButton {
//     Todo,
// }

// #[derive(Debug, Copy, Clone, PartialEq, Eq)]
// #[must_use]
// pub enum ControllerUpdate {
//     Added,
//     Removed,
// }

// /// Indicates an [Axis] direction.
// #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
// #[must_use]
// pub enum AxisDirection {
//     /// No direction, axis is in a deadzone/not pressed.
//     None,
//     /// Positive (Right or Down)
//     Positive,
//     /// Negative (Left or Up)
//     Negative,
// }

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
    // ControllerBtn(InputControllerBtn),
    // ControllerAxis(InputControllerAxis),
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
        key_map!(map, One, Escape, NesState::TogglePause);
        key_map!(map, One, KeyH, CONTROL, Menu::About);
        key_map!(map, One, F1, Menu::About);
        key_map!(map, One, KeyC, CONTROL, Menu::Config(ConfigTab::General));
        key_map!(map, One, F2, Menu::Config(ConfigTab::General));
        key_map!(map, One, KeyO, CONTROL, Menu::LoadRom);
        key_map!(map, One, F3, Menu::LoadRom);
        key_map!(map, One, KeyK, CONTROL, Menu::Keybind(Player::One));
        key_map!(map, One, KeyQ, CONTROL, NesState::Quit);
        key_map!(map, One, KeyR, CONTROL, NesState::SoftReset);
        key_map!(map, One, KeyP, CONTROL, NesState::HardReset);
        key_map!(map, One, Equal, CONTROL, Setting::IncSpeed);
        key_map!(map, One, Minus, CONTROL, Setting::DecSpeed);
        key_map!(map, One, Space, Setting::FastForward);
        key_map!(map, One, Digit1, CONTROL, Setting::SetSaveSlot(1));
        key_map!(map, One, Digit2, CONTROL, Setting::SetSaveSlot(2));
        key_map!(map, One, Digit3, CONTROL, Setting::SetSaveSlot(3));
        key_map!(map, One, Digit4, CONTROL, Setting::SetSaveSlot(4));
        key_map!(map, One, Numpad1, CONTROL, Setting::SetSaveSlot(1));
        key_map!(map, One, Numpad2, CONTROL, Setting::SetSaveSlot(2));
        key_map!(map, One, Numpad3, CONTROL, Setting::SetSaveSlot(3));
        key_map!(map, One, Numpad4, CONTROL, Setting::SetSaveSlot(4));
        key_map!(map, One, KeyS, CONTROL, Feature::SaveState);
        key_map!(map, One, KeyL, CONTROL, Feature::LoadState);
        key_map!(map, One, KeyR, Feature::Rewind);
        key_map!(map, One, F10, Feature::TakeScreenshot);
        key_map!(map, One, KeyV, SHIFT, Feature::ToggleReplayRecord);
        key_map!(map, One, KeyR, SHIFT, Feature::ToggleAudioRecord);
        key_map!(map, One, KeyM, CONTROL, Setting::ToggleAudio);
        key_map!(
            map,
            One,
            Numpad1,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Pulse1)
        );
        key_map!(
            map,
            One,
            Numpad2,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Pulse2)
        );
        key_map!(
            map,
            One,
            Numpad3,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Triangle)
        );
        key_map!(
            map,
            One,
            Numpad4,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Noise)
        );
        key_map!(
            map,
            One,
            Numpad5,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Dmc)
        );
        key_map!(map, One, Enter, CONTROL, Setting::ToggleFullscreen);
        key_map!(map, One, KeyV, CONTROL, Setting::ToggleVsync);
        key_map!(
            map,
            One,
            KeyN,
            CONTROL,
            Setting::ToggleVideoFilter(VideoFilter::Ntsc)
        );
        key_map!(
            map,
            One,
            KeyD,
            SHIFT,
            DebugAction::ToggleDebugger(Debugger::Cpu)
        );
        key_map!(
            map,
            One,
            KeyP,
            SHIFT,
            DebugAction::ToggleDebugger(Debugger::Ppu)
        );
        key_map!(
            map,
            One,
            KeyA,
            SHIFT,
            DebugAction::ToggleDebugger(Debugger::Apu)
        );
        key_map!(map, One, KeyC, DebugAction::Step(Step::Into));
        key_map!(map, One, KeyO, DebugAction::Step(Step::Over));
        key_map!(map, One, KeyO, SHIFT, DebugAction::Step(Step::Out));
        key_map!(map, One, KeyL, SHIFT, DebugAction::Step(Step::Scanline));
        key_map!(map, One, KeyF, SHIFT, DebugAction::Step(Step::Frame));
        key_map!(map, One, ArrowDown, CONTROL, DebugAction::IncScanline);
        key_map!(map, One, ArrowUp, CONTROL, DebugAction::DecScanline);
        key_map!(
            map,
            One,
            ArrowDown,
            SHIFT | CONTROL,
            DebugAction::IncScanline
        );
        key_map!(map, One, ArrowUp, SHIFT | CONTROL, DebugAction::DecScanline);

        mouse_map!(map, Two, MouseButton::Left, Action::ZapperTrigger);

        // TODO: controller bindings
        // controller_bind!(One, ControllerButton::DPadLeft, JoypadBtn::Left),
        // controller_bind!(One, ControllerButton::DPadRight, JoypadBtn::Right),
        // controller_bind!(One, ControllerButton::DPadUp, JoypadBtn::Up),
        // controller_bind!(One, ControllerButton::DPadDown, JoypadBtn::Down),
        // controller_bind!(One, ControllerButton::A, JoypadBtn::A),
        // controller_bind!(One, ControllerButton::B, JoypadBtn::B),
        // controller_bind!(One, ControllerButton::X, JoypadBtn::TurboA),
        // controller_bind!(One, ControllerButton::Y, JoypadBtn::TurboB),
        // controller_bind!(One, ControllerButton::Guide, Menu::Main),
        // controller_bind!(One, ControllerButton::Start, JoypadBtn::Start),
        // controller_bind!(One, ControllerButton::Back, JoypadBtn::Select),
        // controller_bind!(One, ControllerButton::RightShoulder, Setting::IncSpeed),
        // controller_bind!(One, ControllerButton::LeftShoulder, Setting::DecSpeed),
        // controller_axis_bind!(One, Axis::LeftX, Direction::Negative, JoypadBtn::Left),
        // controller_axis_bind!(One, Axis::LeftX, Direction::Positive, JoypadBtn::Right),
        // controller_axis_bind!(One, Axis::LeftY, Direction::Negative, JoypadBtn::Up),
        // controller_axis_bind!(One, Axis::LeftY, Direction::Positive, JoypadBtn::Down),
        // controller_axis_bind!(
        //     One,
        //     Axis::TriggerLeft,
        //     Direction::Positive,
        //     Feature::SaveState
        // ),
        // controller_axis_bind!(
        //     One,
        //     Axis::TriggerRight,
        //     Direction::Positive,
        //     Feature::LoadState
        // ),

        Self(map)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Nes(NesState),
    Menu(Menu),
    Feature(Feature),
    Setting(Setting),
    Joypad(JoypadBtn),
    ZapperTrigger,
    Debug(DebugAction),
}

impl From<NesState> for Action {
    fn from(state: NesState) -> Self {
        Self::Nes(state)
    }
}

impl From<Menu> for Action {
    fn from(menu: Menu) -> Self {
        Self::Menu(menu)
    }
}

impl From<Feature> for Action {
    fn from(feature: Feature) -> Self {
        Self::Feature(feature)
    }
}

impl From<Setting> for Action {
    fn from(setting: Setting) -> Self {
        Self::Setting(setting)
    }
}

impl From<JoypadBtn> for Action {
    fn from(btn: JoypadBtn) -> Self {
        Self::Joypad(btn)
    }
}

impl From<DebugAction> for Action {
    fn from(action: DebugAction) -> Self {
        Self::Debug(action)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum NesState {
    Quit,
    TogglePause,
    SoftReset,
    HardReset,
    MapperRevision(MapperRevision),
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Feature {
    ToggleReplayRecord,
    ToggleAudioRecord,
    Rewind,
    TakeScreenshot,
    SaveState,
    LoadState,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Setting {
    SetSaveSlot(u8),
    ToggleFullscreen,
    ToggleVsync,
    ToggleVideoFilter(VideoFilter),
    SetVideoFilter(VideoFilter),
    SetNesFormat(NesRegion),
    ToggleAudio,
    ToggleApuChannel(Channel),
    FastForward,
    IncSpeed,
    DecSpeed,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum Debugger {
    Cpu,
    Ppu,
    Apu,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum Step {
    Into,
    Out,
    Over,
    Scanline,
    Frame,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum DebugAction {
    ToggleDebugger(Debugger),
    Step(Step),
    IncScanline,
    DecScanline,
}

// const fn render_message(_message: &str, _color: Color) {
//     // TODO: switch to egui
//     // s.push();
//     // s.stroke(None);
//     // s.fill(rgb!(0, 200));
//     // let pady = s.theme().spacing.frame_pad.y();
//     // let width = s.width()?;
//     // s.wrap(width);
//     // let (_, height) = s.size_of(message)?;
//     // s.rect([
//     //     0,
//     //     s.cursor_pos().y() - pady,
//     //     width as i32,
//     //     height as i32 + 2 * pady,
//     // ])?;
//     // s.fill(color);
//     // s.text(message)?;
//     // s.pop();
// }

// impl Nes {
//
//     pub fn handle_controller_update(&mut self, device_id: DeviceId, update: ControllerUpdate) {
//         match update {
//             ControllerUpdate::Added => {
//                 for player in [Slot::One, Slot::Two, Slot::Three, Slot::Four] {
//                     let player_idx = player as usize;
//                     if self.controllers[player_idx].is_none() {
//                         self.add_message(format!("Controller {} connected.", player_idx + 1));
//                         self.controllers[player_idx] = Some(device_id);
//                     }
//                 }
//             }
//             ControllerUpdate::Removed => {
//                 if let Some(player) = self.get_controller_player(device_id) {
//                     let player_idx = player as usize;
//                     self.controllers[player_idx] = None;
//                     self.add_message(format!("Controller {} disconnected.", player_idx + 1));
//                 }
//             }
//         }
//     }

//
//     pub fn handle_controller_event(
//         &mut self,
//         device_id: DeviceId,
//         button_id: ButtonId,
//         pressed: bool,
//     ) {
//         if let Some(player) = self.get_controller_player(device_id) {
//             self.handle_input(
//                 player,
//                 Input::ControllerBtn(InputControllerBtn::new(player, button_id)),
//                 pressed,
//                 false,
//             );
//         }
//     }

//
//     pub fn handle_controller_axis_motion(&mut self, device_id: DeviceId, axis: AxisId, value: f64) {
//         if let Some(player) = self.get_controller_player(device_id) {
//             let direction = if value < self.config.controller_deadzone {
//                 AxisDirection::Negative
//             } else if value > self.config.controller_deadzone {
//                 AxisDirection::Positive
//             } else {
//                 // TODO: verify if this is correct
//                 for button in [
//                     JoypadBtn::Left,
//                     JoypadBtn::Right,
//                     JoypadBtn::Up,
//                     JoypadBtn::Down,
//                 ] {
//                     self.handle_joypad_pressed(player, button, false);
//                 }
//                 return;
//             };
//             self.handle_input(
//                 player,
//                 Input::ControllerAxis(InputControllerAxis::new(player, axis, direction)),
//                 true,
//                 false,
//             );
//         }
//     }

// }

// impl Nes {
//
//     fn get_controller_player(&self, device_id: DeviceId) -> Option<Slot> {
//         self.controllers.iter().enumerate().find_map(|(player, id)| {
//             (*id == Some(device_id)).then_some(Slot::try_from(player).expect("valid player index"))
//         })
//     }

//     // fn debug_step_into(&mut self) {
//     //     self.pause_play(PauseMode::Manual);
//     //     if let Err(err) = self.control_deck.clock_instr() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     // }

//     // fn next_instr(&mut self) -> Instr {
//     //     let pc = self.control_deck.cpu().pc();
//     //     let opcode = self.control_deck.cpu().peek(pc, Access::Dummy);
//     //     Cpu::INSTRUCTIONS[opcode as usize]
//     // }

//     // fn debug_step_over(&mut self) {
//     //     self.pause_play(PauseMode::Manual);
//     //     let instr = self.next_instr();
//     //     if let Err(err) = self.control_deck.clock_instr() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     //     if instr.op() == Operation::JSR {
//     //         let rti_addr = self.control_deck.cpu().peek_stack_u16().wrapping_add(1);
//     //         while self.control_deck.cpu().pc() != rti_addr {
//     //             if let Err(err) = self.control_deck.clock_instr() {
//     //                 self.handle_emulation_error(&err);
//     //                 break;
//     //             }
//     //         }
//     //     }
//     // }

//     // fn debug_step_out(&mut self) {
//     //     let mut instr = self.next_instr();
//     //     while !matches!(instr.op(), Operation::RTS | Operation::RTI) {
//     //         if let Err(err) = self.control_deck.clock_instr() {
//     //             self.handle_emulation_error(&err);
//     //             break;
//     //         }
//     //         instr = self.next_instr();
//     //     }
//     //     if let Err(err) = self.control_deck.clock_instr() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     // }

//     // fn debug_step_frame(&mut self) {
//     //     self.pause_play(PauseMode::Manual);
//     //     if let Err(err) = self.control_deck.clock_frame() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     // }

//     // fn debug_step_scanline(&mut self) {
//     //     self.pause_play(PauseMode::Manual);
//     //     if let Err(err) = self.control_deck.clock_scanline() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     // }
// }
