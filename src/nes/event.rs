use crate::{
    apu::Channel,
    common::{Kind, NesRegion, Reset},
    input::{JoypadBtn, JoypadBtnState, Slot},
    mapper::MapperRevision,
    nes::{
        menu::{types::ConfigSection, Menu, Player},
        state::{Mode, ReplayMode},
        Nes, PauseMode,
    },
    video::VideoFilter,
};
use pixels::wgpu::Color;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt,
    ops::{Deref, DerefMut},
    time::Duration,
};
use web_time::Instant;
use winit::{
    dpi::PhysicalPosition,
    event::{AxisId, ButtonId, DeviceId, ElementState, KeyEvent, MouseButton},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{Fullscreen, WindowId},
};

// #[derive(Debug, Copy, Clone, PartialEq)]
// #[must_use]
// pub struct DeviceId(usize);

// #[derive(Debug, Copy, Clone, PartialEq, Eq)]
// #[must_use]
// pub enum ControllerButton {
//     Todo,
// }

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum ControllerUpdate {
    Added,
    Removed,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[must_use]
pub enum CustomEvent {
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

/// Indicates an [Axis] direction.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum AxisDirection {
    /// No direction, axis is in a deadzone/not pressed.
    None,
    /// Positive (Right or Down)
    Positive,
    /// Negative (Left or Up)
    Negative,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct ActionEvent {
    pub frame: u32,
    pub slot: Slot,
    pub action: Action,
    pub pressed: bool,
    pub repeat: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct InputKey {
    pub controller: Slot,
    pub key: KeyCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<ModifiersState>,
}

impl InputKey {
    pub const fn new(controller: Slot, key: KeyCode, modifiers: ModifiersState) -> Self {
        Self {
            controller,
            key,
            modifiers: if modifiers.is_empty() {
                None
            } else {
                Some(modifiers)
            },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct InputMouse {
    pub controller: Slot,
    pub button: MouseButton,
}

impl InputMouse {
    pub const fn new(controller: Slot, button: MouseButton) -> Self {
        Self { controller, button }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct InputControllerBtn {
    pub controller: Slot,
    pub button_id: ButtonId,
}

impl InputControllerBtn {
    pub const fn new(controller: Slot, button_id: ButtonId) -> Self {
        Self {
            controller,
            button_id,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct InputControllerAxis {
    pub controller: Slot,
    pub axis_id: AxisId,
    pub direction: AxisDirection,
}

impl InputControllerAxis {
    pub const fn new(controller: Slot, axis_id: AxisId, direction: AxisDirection) -> Self {
        Self {
            controller,
            axis_id,
            direction,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Input {
    Key(InputKey),
    Mouse(InputMouse),
    ControllerBtn(InputControllerBtn),
    ControllerAxis(InputControllerAxis),
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Input::Key(InputKey {
                controller,
                key,
                modifiers,
            }) => {
                if modifiers.map_or(true, |modifiers| modifiers.is_empty()) {
                    write!(f, "{controller:?} {key:?}")
                } else {
                    write!(f, "{controller:?} {modifiers:?} {key:?}")
                }
            }
            Input::Mouse(InputMouse { controller, button }) => {
                write!(f, "{controller:?} {button:?}")
            }
            Input::ControllerBtn(InputControllerBtn {
                controller,
                button_id,
            }) => write!(f, "{controller:?} {button_id:?}"),
            Input::ControllerAxis(InputControllerAxis {
                controller,
                axis_id,
                direction,
            }) => write!(f, "{controller:?} {axis_id:?} {direction:?}"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct KeyBinding {
    pub input: InputKey,
    pub action: Action,
}

impl KeyBinding {
    pub const fn new(input: InputKey, action: Action) -> Self {
        Self { input, action }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct MouseBinding {
    pub input: InputMouse,
    pub action: Action,
}

impl MouseBinding {
    pub const fn new(input: InputMouse, action: Action) -> Self {
        Self { input, action }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct ControllerButtonBinding {
    pub input: InputControllerBtn,
    pub action: Action,
}

impl ControllerButtonBinding {
    pub const fn new(input: InputControllerBtn, action: Action) -> Self {
        Self { input, action }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct ControllerAxisBinding {
    pub input: InputControllerAxis,
    pub action: Action,
}

impl ControllerAxisBinding {
    pub const fn new(input: InputControllerAxis, action: Action) -> Self {
        Self { input, action }
    }
}

/// A binding of a inputs to an [Action].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputBindings {
    pub keys: Vec<KeyBinding>,
    pub mouse: Vec<MouseBinding>,
    pub controller_btns: Vec<ControllerButtonBinding>,
    pub controller_axes: Vec<ControllerAxisBinding>,
}

impl InputBindings {
    pub(crate) fn set(&mut self, input: Input, action: Action) {
        match input {
            Input::Key(input) => {
                if !self.keys.iter().any(|b| b.input == input) {
                    self.keys.push(KeyBinding::new(input, action));
                }
            }
            Input::Mouse(input) => {
                if !self.mouse.iter().any(|b| b.input == input) {
                    self.mouse.push(MouseBinding::new(input, action));
                }
            }
            Input::ControllerBtn(input) => {
                if !self.controller_btns.iter().any(|b| b.input == input) {
                    self.controller_btns
                        .push(ControllerButtonBinding::new(input, action));
                }
            }
            Input::ControllerAxis(input) => {
                if !self.controller_axes.iter().any(|b| b.input == input) {
                    self.controller_axes
                        .push(ControllerAxisBinding::new(input, action));
                }
            }
        }
    }

    pub(crate) fn unset(&mut self, input: Input) {
        match input {
            Input::Key(input) => self.keys.retain(|b| b.input != input),
            Input::Mouse(input) => self.mouse.retain(|b| b.input != input),
            Input::ControllerBtn(input) => self.controller_btns.retain(|b| b.input != input),
            Input::ControllerAxis(input) => self.controller_axes.retain(|b| b.input != input),
        }
    }
}

macro_rules! key_bind {
    ($slot:expr, $key:expr, $action:expr) => {
        KeyBinding::new(
            InputKey::new($slot, $key, ModifiersState::empty()),
            $action.into(),
        )
    };
    ($slot:expr, $key:expr, $modifiers:expr, $action:expr) => {
        KeyBinding::new(InputKey::new($slot, $key, $modifiers), $action.into())
    };
}

macro_rules! mouse_bind {
    ($slot:expr, $button:expr, $action:expr) => {
        MouseBinding::new(InputMouse::new($slot, $button), $action.into())
    };
}

// macro_rules! controller_bind {
//     ($slot:expr, $button_id:expr, $action:expr) => {
//         ControllerBinding::new(InputControllerBtn::new($slot, $button_id), $action.into())
//     };
// }

// macro_rules! controller_axis_bind {
//     ($slot:expr, $button_id:expr, $direction:expr, $action:expr) => {
//         ControllerAxisBinding::new(
//             InputControllerAxis::new($slot, $button_id, $direction),
//             $action.into(),
//         )
//     };
// }

impl Default for InputBindings {
    fn default() -> Self {
        use ConfigSection::*;
        use KeyCode::*;
        use Slot::*;
        const SHIFT: ModifiersState = ModifiersState::SHIFT;
        const CONTROL: ModifiersState = ModifiersState::CONTROL;

        Self {
            keys: vec![
                key_bind!(One, ArrowLeft, JoypadBtn::Left),
                key_bind!(One, ArrowRight, JoypadBtn::Right),
                key_bind!(One, ArrowUp, JoypadBtn::Up),
                key_bind!(One, ArrowDown, JoypadBtn::Down),
                key_bind!(One, KeyZ, JoypadBtn::A),
                key_bind!(One, KeyX, JoypadBtn::B),
                key_bind!(One, KeyA, JoypadBtn::TurboA),
                key_bind!(One, KeyS, JoypadBtn::TurboB),
                key_bind!(One, Enter, JoypadBtn::Start),
                key_bind!(One, ShiftRight, JoypadBtn::Select),
                key_bind!(One, ShiftLeft, JoypadBtn::Select),
                key_bind!(Two, KeyJ, JoypadBtn::Left),
                key_bind!(Two, KeyL, JoypadBtn::Right),
                key_bind!(Two, KeyI, JoypadBtn::Up),
                key_bind!(Two, KeyK, JoypadBtn::Down),
                key_bind!(Two, KeyN, JoypadBtn::A),
                key_bind!(Two, KeyM, JoypadBtn::B),
                key_bind!(Two, Numpad8, JoypadBtn::Start),
                key_bind!(Two, Numpad9, SHIFT, JoypadBtn::Select),
                key_bind!(Three, KeyF, JoypadBtn::Left),
                key_bind!(Three, KeyH, JoypadBtn::Right),
                key_bind!(Three, KeyT, JoypadBtn::Up),
                key_bind!(Three, KeyG, JoypadBtn::Down),
                key_bind!(Three, KeyV, JoypadBtn::A),
                key_bind!(Three, KeyB, JoypadBtn::B),
                key_bind!(Three, Numpad5, JoypadBtn::Start),
                key_bind!(Three, Numpad6, SHIFT, JoypadBtn::Select),
                key_bind!(One, Escape, NesState::TogglePause),
                key_bind!(One, KeyH, CONTROL, Menu::About),
                key_bind!(One, F1, Menu::About),
                key_bind!(One, KeyC, CONTROL, Menu::Config(General)),
                key_bind!(One, F2, Menu::Config(General)),
                key_bind!(One, KeyO, CONTROL, Menu::LoadRom),
                key_bind!(One, F3, Menu::LoadRom),
                key_bind!(One, KeyK, CONTROL, Menu::Keybind(Player::One)),
                key_bind!(One, KeyQ, CONTROL, NesState::Quit),
                key_bind!(One, KeyR, CONTROL, NesState::SoftReset),
                key_bind!(One, KeyP, CONTROL, NesState::HardReset),
                key_bind!(One, Equal, CONTROL, Setting::IncSpeed),
                key_bind!(One, Minus, CONTROL, Setting::DecSpeed),
                key_bind!(One, Space, Setting::FastForward),
                key_bind!(One, Digit1, CONTROL, Setting::SetSaveSlot(1)),
                key_bind!(One, Digit2, CONTROL, Setting::SetSaveSlot(2)),
                key_bind!(One, Digit3, CONTROL, Setting::SetSaveSlot(3)),
                key_bind!(One, Digit4, CONTROL, Setting::SetSaveSlot(4)),
                key_bind!(One, Numpad1, CONTROL, Setting::SetSaveSlot(1)),
                key_bind!(One, Numpad2, CONTROL, Setting::SetSaveSlot(2)),
                key_bind!(One, Numpad3, CONTROL, Setting::SetSaveSlot(3)),
                key_bind!(One, Numpad4, CONTROL, Setting::SetSaveSlot(4)),
                key_bind!(One, KeyS, CONTROL, Feature::SaveState),
                key_bind!(One, KeyL, CONTROL, Feature::LoadState),
                key_bind!(One, KeyR, Feature::Rewind),
                key_bind!(One, F10, Feature::TakeScreenshot),
                key_bind!(One, KeyV, SHIFT, Feature::ToggleGameplayRecording),
                key_bind!(One, KeyR, SHIFT, Feature::ToggleSoundRecording),
                key_bind!(One, KeyM, CONTROL, Setting::ToggleSound),
                key_bind!(One, Numpad1, SHIFT, Setting::TogglePulse1),
                key_bind!(One, Numpad2, SHIFT, Setting::TogglePulse2),
                key_bind!(One, Numpad3, SHIFT, Setting::ToggleTriangle),
                key_bind!(One, Numpad4, SHIFT, Setting::ToggleNoise),
                key_bind!(One, Numpad5, SHIFT, Setting::ToggleDmc),
                key_bind!(One, Enter, CONTROL, Setting::ToggleFullscreen),
                key_bind!(One, KeyV, CONTROL, Setting::ToggleVsync),
                key_bind!(One, KeyN, CONTROL, Setting::ToggleNtscFilter),
                key_bind!(One, KeyD, SHIFT, DebugAction::ToggleCpuDebugger),
                key_bind!(One, KeyP, SHIFT, DebugAction::TogglePpuDebugger),
                key_bind!(One, KeyA, SHIFT, DebugAction::ToggleApuDebugger),
                key_bind!(One, KeyC, DebugAction::StepInto),
                key_bind!(One, KeyO, DebugAction::StepOver),
                key_bind!(One, KeyO, SHIFT, DebugAction::StepOut),
                key_bind!(One, KeyL, SHIFT, DebugAction::StepScanline),
                key_bind!(One, KeyF, SHIFT, DebugAction::StepFrame),
                key_bind!(One, ArrowDown, CONTROL, DebugAction::IncScanline),
                key_bind!(One, ArrowUp, CONTROL, DebugAction::DecScanline),
                key_bind!(One, ArrowDown, SHIFT | CONTROL, DebugAction::IncScanline),
                key_bind!(One, ArrowUp, SHIFT | CONTROL, DebugAction::DecScanline),
            ],
            mouse: vec![mouse_bind!(Two, MouseButton::Left, Action::ZapperTrigger)],
            controller_btns: vec![],
            // TODO: Controller bindings
            // buttons: vec![
            //     controller_bind!(One, ControllerButton::DPadLeft, JoypadBtn::Left),
            //     controller_bind!(One, ControllerButton::DPadRight, JoypadBtn::Right),
            //     controller_bind!(One, ControllerButton::DPadUp, JoypadBtn::Up),
            //     controller_bind!(One, ControllerButton::DPadDown, JoypadBtn::Down),
            //     controller_bind!(One, ControllerButton::A, JoypadBtn::A),
            //     controller_bind!(One, ControllerButton::B, JoypadBtn::B),
            //     controller_bind!(One, ControllerButton::X, JoypadBtn::TurboA),
            //     controller_bind!(One, ControllerButton::Y, JoypadBtn::TurboB),
            //     controller_bind!(One, ControllerButton::Guide, Menu::Main),
            //     controller_bind!(One, ControllerButton::Start, JoypadBtn::Start),
            //     controller_bind!(One, ControllerButton::Back, JoypadBtn::Select),
            //     controller_bind!(One, ControllerButton::RightShoulder, Setting::IncSpeed),
            //     controller_bind!(One, ControllerButton::LeftShoulder, Setting::DecSpeed),
            // ],
            controller_axes: vec![],
            // TODO: Axis bindings
            // axes: vec![
            //     controller_axis_bind!(One, Axis::LeftX, Direction::Negative, JoypadBtn::Left),
            //     controller_axis_bind!(One, Axis::LeftX, Direction::Positive, JoypadBtn::Right),
            //     controller_axis_bind!(One, Axis::LeftY, Direction::Negative, JoypadBtn::Up),
            //     controller_axis_bind!(One, Axis::LeftY, Direction::Positive, JoypadBtn::Down),
            //     controller_axis_bind!(
            //         One,
            //         Axis::TriggerLeft,
            //         Direction::Positive,
            //         Feature::SaveState
            //     ),
            //     controller_axis_bind!(
            //         One,
            //         Axis::TriggerRight,
            //         Direction::Positive,
            //         Feature::LoadState
            //     ),
            // ],
        }
    }
}
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct InputMapping(HashMap<Input, Action>);

impl InputMapping {
    #[must_use]
    pub fn from_bindings(bindings: &InputBindings) -> Self {
        let mut input_map = Self::default();
        for bind in &bindings.keys {
            input_map.insert(Input::Key(bind.input), bind.action);
        }
        for bind in &bindings.mouse {
            input_map.insert(Input::Mouse(bind.input), bind.action);
        }
        for bind in &bindings.controller_btns {
            input_map.insert(Input::ControllerBtn(bind.input), bind.action);
        }
        for bind in &bindings.controller_axes {
            input_map.insert(Input::ControllerAxis(bind.input), bind.action);
        }
        input_map
    }
}

impl Deref for InputMapping {
    type Target = HashMap<Input, Action>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for InputMapping {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[allow(variant_size_differences)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NesState {
    Quit,
    TogglePause,
    SoftReset,
    HardReset,
    MapperRevision(MapperRevision),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Feature {
    ToggleGameplayRecording,
    ToggleSoundRecording,
    Rewind,
    TakeScreenshot,
    SaveState,
    LoadState,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Setting {
    SetSaveSlot(u8),
    ToggleFullscreen,
    ToggleVsync,
    ToggleNtscFilter,
    SetVideoFilter(VideoFilter),
    SetNesFormat(NesRegion),
    ToggleSound,
    TogglePulse1,
    TogglePulse2,
    ToggleTriangle,
    ToggleNoise,
    ToggleDmc,
    FastForward,
    IncSpeed,
    DecSpeed,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DebugAction {
    ToggleCpuDebugger,
    TogglePpuDebugger,
    ToggleApuDebugger,
    StepInto,
    StepOver,
    StepOut,
    StepFrame,
    StepScanline,
    IncScanline,
    DecScanline,
}

const fn render_message(_message: &str, _color: Color) {
    // TODO: switch to egui
    // s.push();
    // s.stroke(None);
    // s.fill(rgb!(0, 200));
    // let pady = s.theme().spacing.frame_pad.y();
    // let width = s.width()?;
    // s.wrap(width);
    // let (_, height) = s.size_of(message)?;
    // s.rect([
    //     0,
    //     s.cursor_pos().y() - pady,
    //     width as i32,
    //     height as i32 + 2 * pady,
    // ])?;
    // s.fill(color);
    // s.text(message)?;
    // s.pop();
}

impl Nes {
    #[inline]
    pub fn add_message<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        let text = text.into();
        log::info!("{text}");
        self.messages.push((text, Instant::now()));
    }

    pub fn render_messages(&mut self) {
        const TIMEOUT: Duration = Duration::from_secs(3);

        let now = Instant::now();
        self.messages
            .retain(|(_, created)| (now - *created) < TIMEOUT);
        self.messages.dedup_by(|a, b| a.0.eq(&b.0));
        for (message, _) in &self.messages {
            render_message(message, Color::WHITE);
        }
    }

    pub fn render_confirm_quit(&mut self) {
        // TODO switch to egui
        // if let Some((ref msg, ref mut confirm)) = self.confirm_quit {
        //     s.push();
        //     s.stroke(None);
        //     s.fill(rgb!(0, 200));
        //     let pady = s.theme().spacing.frame_pad.y();
        //     let width = s.width()?;
        //     s.wrap(width);
        //     let (_, height) = s.size_of(msg)?;
        //     s.rect([
        //         0,
        //         s.cursor_pos().y() - pady,
        //         width as i32,
        //         4 * height as i32 + 2 * pady,
        //     ])?;
        //     s.fill(Color::WHITE);
        //     s.text(msg)?;
        //     if s.button("Confirm")? {
        //         *confirm = true;
        //         s.pop();
        //         return Ok(true);
        //     }
        //     s.same_line(None);
        //     if s.button("Cancel")? {
        //         self.confirm_quit = None;
        //         self.resume_play();
        //     }
        //     s.pop();
        // }
    }

    #[inline]
    pub fn render_status(&mut self, status: &str) {
        render_message(status, Color::WHITE);
        if let Some(ref err) = self.error {
            render_message(err, Color::RED);
        }
    }

    #[inline]
    pub fn handle_input(&mut self, slot: Slot, input: Input, pressed: bool, repeat: bool) {
        if let Some(action) = self.config.input_map.get(&input).copied() {
            self.handle_action(slot, action, pressed, repeat);
        }
    }

    pub fn handle_key_event(&mut self, _window_id: WindowId, event: KeyEvent) {
        if let PhysicalKey::Code(key) = event.physical_key {
            for slot in [Slot::One, Slot::Two, Slot::Three, Slot::Four] {
                self.handle_input(
                    slot,
                    Input::Key(InputKey::new(slot, key, self.modifiers.state())),
                    event.state.is_pressed(),
                    event.repeat,
                );
            }
            // FIXME: DEBUG
            if self.modifiers.lshift_state() == winit::keyboard::ModifiersKeyState::Pressed {
                match key {
                    KeyCode::ArrowUp if !event.state.is_pressed() => {
                        if self.config.audio_latency <= Duration::from_millis(90) {
                            self.config.audio_latency += Duration::from_millis(10);
                            log::info!("Audio delay time: {:?}", self.config.audio_latency);
                            let _ = self.audio.set_audio_latency(self.config.audio_latency);
                        }
                    }
                    KeyCode::ArrowDown if !event.state.is_pressed() => {
                        if self.config.audio_latency >= Duration::from_millis(20) {
                            self.config.audio_latency -= Duration::from_millis(10);
                            log::info!("Audio delay time: {:?}", self.config.audio_latency);
                            let _ = self.audio.set_audio_latency(self.config.audio_latency);
                        }
                    }
                    _ => (),
                }
            }
        }
    }

    pub fn handle_mouse_event(
        &mut self,
        _window_id: WindowId,
        btn: MouseButton,
        state: ElementState,
    ) -> bool {
        // To avoid consuming events while in menus
        if self.mode.is_playing() {
            for slot in [Slot::One, Slot::Two] {
                self.handle_input(
                    slot,
                    Input::Mouse(InputMouse::new(slot, btn)),
                    state.is_pressed(),
                    false,
                );
            }
        }
        false
    }

    #[inline]
    fn handle_zapper_trigger(&mut self) {
        self.control_deck.trigger_zapper();
    }

    pub fn set_zapper_pos(&mut self, pos: PhysicalPosition<f64>) {
        let x = (pos.x / self.config.scale as f64) * 8.0 / 7.0 + 0.5; // Adjust ratio
        let mut y = pos.y / self.config.scale as f64;
        // Account for trimming top 8 scanlines
        if self.config.region.is_ntsc() {
            y += 8.0;
        };
        self.control_deck
            .aim_zapper(x.round() as i32, y.round() as i32);
    }

    #[inline]
    pub fn handle_mouse_motion(
        &mut self,
        _window_id: WindowId,
        pos: PhysicalPosition<f64>,
    ) -> bool {
        // To avoid consuming events while in menus
        if self.mode.is_playing() {
            self.set_zapper_pos(pos);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn handle_controller_update(&mut self, device_id: DeviceId, update: ControllerUpdate) {
        match update {
            ControllerUpdate::Added => {
                for slot in [Slot::One, Slot::Two, Slot::Three, Slot::Four] {
                    let slot_idx = slot as usize;
                    if self.controllers[slot_idx].is_none() {
                        self.add_message(format!("Controller {} connected.", slot_idx + 1));
                        self.controllers[slot_idx] = Some(device_id);
                    }
                }
            }
            ControllerUpdate::Removed => {
                if let Some(slot) = self.get_controller_slot(device_id) {
                    let slot_idx = slot as usize;
                    self.controllers[slot_idx] = None;
                    self.add_message(format!("Controller {} disconnected.", slot_idx + 1));
                }
            }
        }
    }

    #[inline]
    pub fn handle_controller_event(
        &mut self,
        device_id: DeviceId,
        button_id: ButtonId,
        pressed: bool,
    ) {
        if let Some(slot) = self.get_controller_slot(device_id) {
            self.handle_input(
                slot,
                Input::ControllerBtn(InputControllerBtn::new(slot, button_id)),
                pressed,
                false,
            );
        }
    }

    #[inline]
    pub fn handle_controller_axis_motion(&mut self, device_id: DeviceId, axis: AxisId, value: f64) {
        if let Some(slot) = self.get_controller_slot(device_id) {
            let direction = if value < self.config.controller_deadzone {
                AxisDirection::Negative
            } else if value > self.config.controller_deadzone {
                AxisDirection::Positive
            } else {
                // TODO: verify if this is correct
                for button in [
                    JoypadBtn::Left,
                    JoypadBtn::Right,
                    JoypadBtn::Up,
                    JoypadBtn::Down,
                ] {
                    self.handle_joypad_pressed(slot, button, false);
                }
                return;
            };
            self.handle_input(
                slot,
                Input::ControllerAxis(InputControllerAxis::new(slot, axis, direction)),
                true,
                false,
            );
        }
    }

    pub fn handle_action(&mut self, slot: Slot, action: Action, pressed: bool, repeat: bool) {
        match action {
            Action::Debug(action) if pressed => self.handle_debug(action, repeat),
            Action::Feature(feature) => self.handle_feature(feature, pressed, repeat),
            Action::Nes(state) if pressed => self.handle_nes_state(state),
            Action::Menu(menu) if pressed => self.toggle_menu(menu),
            Action::Setting(setting) => self.handle_setting(setting, pressed, repeat),
            Action::Joypad(button) => self.handle_joypad_pressed(slot, button, pressed),
            Action::ZapperTrigger if pressed => self.handle_zapper_trigger(),
            _ => (),
        }

        if self.replay.mode == ReplayMode::Recording {
            self.replay
                .buffer
                .push(self.action_event(slot, action, pressed, repeat));
        }
    }

    pub fn replay_action(&mut self) {
        let current_frame = self.control_deck.frame_number();
        while let Some(action_event) = self.replay.buffer.last() {
            match action_event.frame.cmp(&current_frame) {
                Ordering::Equal => {
                    let ActionEvent {
                        slot,
                        action,
                        pressed,
                        repeat,
                        ..
                    } = self.replay.buffer.pop().expect("valid action event");
                    self.handle_action(slot, action, pressed, repeat);
                }
                Ordering::Less => {
                    log::warn!(
                        "Encountered action event out of order: {} < {}",
                        action_event.frame,
                        current_frame
                    );
                    self.replay.buffer.pop();
                }
                Ordering::Greater => break,
            }
        }
        if self.replay.buffer.is_empty() {
            self.stop_replay();
        }
    }
}

impl Nes {
    #[inline]
    const fn action_event(
        &self,
        slot: Slot,
        action: Action,
        pressed: bool,
        repeat: bool,
    ) -> ActionEvent {
        ActionEvent {
            frame: self.control_deck.frame_number(),
            slot,
            action,
            pressed,
            repeat,
        }
    }

    #[inline]
    fn get_controller_slot(&self, device_id: DeviceId) -> Option<Slot> {
        self.controllers.iter().enumerate().find_map(|(slot, id)| {
            (*id == Some(device_id)).then_some(Slot::try_from(slot).expect("valid slot index"))
        })
    }

    fn handle_nes_state(&mut self, state: NesState) {
        if self.replay.mode == ReplayMode::Recording {
            return;
        }
        match state {
            NesState::Quit => {
                self.pause_play(PauseMode::Manual);
                self.quitting = true;
            }
            NesState::TogglePause => self.toggle_pause(),
            NesState::SoftReset => {
                self.error = None;
                self.control_deck.reset(Kind::Soft);
                self.add_message("Reset");
                // TODO: add debugger
                // if self.debugger.is_some() && self.mode != Mode::Paused {
                //     self.mode = Mode::Paused;
                // }
            }
            NesState::HardReset => {
                self.error = None;
                self.control_deck.reset(Kind::Hard);
                self.add_message("Power Cycled");
                // TODO: add debugger
                // if self.debugger.is_some() {
                //     self.mode = Mode::Paused;
                // }
            }
            NesState::MapperRevision(_) => todo!("mapper revision"),
        }
    }

    fn handle_feature(&mut self, feature: Feature, pressed: bool, repeat: bool) {
        if feature == Feature::Rewind {
            if repeat {
                if self.config.rewind {
                    self.mode = Mode::Rewinding;
                } else {
                    self.add_message("Rewind disabled. You can enable it in the Config menu.");
                }
            } else if !pressed {
                if self.mode.is_rewinding() {
                    self.resume_play();
                } else {
                    self.instant_rewind();
                }
            }
        } else if pressed {
            match feature {
                Feature::ToggleGameplayRecording => match self.replay.mode {
                    ReplayMode::Off => self.start_replay(),
                    ReplayMode::Recording | ReplayMode::Playback => self.stop_replay(),
                },
                Feature::ToggleSoundRecording => self.toggle_sound_recording(),
                Feature::TakeScreenshot => self.save_screenshot(),
                Feature::SaveState => self.save_state(self.config.save_slot),
                Feature::LoadState => self.load_state(self.config.save_slot),
                Feature::Rewind => (), // Handled above
            }
        }
    }

    fn handle_setting(&mut self, setting: Setting, pressed: bool, _repeat: bool) {
        if setting == Setting::FastForward {
            if pressed {
                self.set_speed(2.0);
            } else if !pressed {
                self.set_speed(1.0);
            }
        } else if pressed {
            match setting {
                Setting::SetSaveSlot(slot) => {
                    self.config.save_slot = slot;
                    self.add_message(&format!("Set Save Slot to {slot}"));
                }
                Setting::ToggleFullscreen => {
                    self.config.fullscreen = !self.config.fullscreen;
                    self.window.set_fullscreen(
                        self.config
                            .fullscreen
                            .then_some(Fullscreen::Borderless(None)),
                    );
                }
                // Vsync is always on in wasm
                Setting::ToggleVsync => {
                    #[cfg(not(target_arch = "wasm32"))]
                    self.set_vsync(self.config.vsync);
                }
                Setting::ToggleNtscFilter => {
                    self.config.filter = match self.config.filter {
                        VideoFilter::Pixellate => VideoFilter::Ntsc,
                        VideoFilter::Ntsc => VideoFilter::Pixellate,
                    };
                    self.control_deck.set_filter(self.config.filter);
                }
                Setting::ToggleSound => {
                    self.config.audio_enabled = !self.config.audio_enabled;
                    self.audio.set_enabled(self.config.audio_enabled);
                    if self.config.audio_enabled {
                        self.add_message("Sound Enabled");
                    } else {
                        self.add_message("Sound Disabled");
                    }
                }
                Setting::TogglePulse1 => self.control_deck.toggle_channel(Channel::Pulse1),
                Setting::TogglePulse2 => self.control_deck.toggle_channel(Channel::Pulse2),
                Setting::ToggleTriangle => self.control_deck.toggle_channel(Channel::Triangle),
                Setting::ToggleNoise => self.control_deck.toggle_channel(Channel::Noise),
                Setting::ToggleDmc => self.control_deck.toggle_channel(Channel::Dmc),
                Setting::IncSpeed => self.change_speed(0.25),
                Setting::DecSpeed => self.change_speed(-0.25),
                // Toggling fast forward happens on key release
                _ => (),
            }
        }
    }

    fn handle_joypad_pressed(&mut self, slot: Slot, button: JoypadBtn, pressed: bool) {
        if !self.mode.is_playing() {
            return;
        }
        let joypad = self.control_deck.joypad_mut(slot);
        if !self.config.concurrent_dpad && pressed {
            match button {
                JoypadBtn::Left => joypad.set_button(JoypadBtnState::RIGHT, false),
                JoypadBtn::Right => joypad.set_button(JoypadBtnState::LEFT, false),
                JoypadBtn::Up => joypad.set_button(JoypadBtnState::DOWN, false),
                JoypadBtn::Down => joypad.set_button(JoypadBtnState::UP, false),
                _ => (),
            }
        }
        joypad.set_button(button.into(), pressed);

        // Ensure that primary button isn't stuck pressed
        match button {
            JoypadBtn::TurboA => joypad.set_button(JoypadBtnState::A, pressed),
            JoypadBtn::TurboB => joypad.set_button(JoypadBtnState::B, pressed),
            _ => (),
        };
    }

    fn handle_debug(&mut self, action: DebugAction, _repeat: bool) {
        // TODO: add debugger
        // let debugging = self.debugger.is_some();
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

    // fn debug_step_into(&mut self) {
    //     self.pause_play(PauseMode::Manual);
    //     if let Err(err) = self.control_deck.clock_instr() {
    //         self.handle_emulation_error(&err);
    //     }
    // }

    // fn next_instr(&mut self) -> Instr {
    //     let pc = self.control_deck.cpu().pc();
    //     let opcode = self.control_deck.cpu().peek(pc, Access::Dummy);
    //     Cpu::INSTRUCTIONS[opcode as usize]
    // }

    // fn debug_step_over(&mut self) {
    //     self.pause_play(PauseMode::Manual);
    //     let instr = self.next_instr();
    //     if let Err(err) = self.control_deck.clock_instr() {
    //         self.handle_emulation_error(&err);
    //     }
    //     if instr.op() == Operation::JSR {
    //         let rti_addr = self.control_deck.cpu().peek_stack_u16().wrapping_add(1);
    //         while self.control_deck.cpu().pc() != rti_addr {
    //             if let Err(err) = self.control_deck.clock_instr() {
    //                 self.handle_emulation_error(&err);
    //                 break;
    //             }
    //         }
    //     }
    // }

    // fn debug_step_out(&mut self) {
    //     let mut instr = self.next_instr();
    //     while !matches!(instr.op(), Operation::RTS | Operation::RTI) {
    //         if let Err(err) = self.control_deck.clock_instr() {
    //             self.handle_emulation_error(&err);
    //             break;
    //         }
    //         instr = self.next_instr();
    //     }
    //     if let Err(err) = self.control_deck.clock_instr() {
    //         self.handle_emulation_error(&err);
    //     }
    // }

    // fn debug_step_frame(&mut self) {
    //     self.pause_play(PauseMode::Manual);
    //     if let Err(err) = self.control_deck.clock_frame() {
    //         self.handle_emulation_error(&err);
    //     }
    // }

    // fn debug_step_scanline(&mut self) {
    //     self.pause_play(PauseMode::Manual);
    //     if let Err(err) = self.control_deck.clock_scanline() {
    //         self.handle_emulation_error(&err);
    //     }
    // }
}
