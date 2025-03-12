use crate::nes::{
    action::{Action, Debug, DebugKind, DebugStep, Feature, Setting, Ui},
    config::{Config, InputConfig},
    renderer::gui::Menu,
};
use egui::ahash::HashMap;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    iter::Peekable,
    ops::{Deref, DerefMut},
};
use tetanes_core::{
    action::Action as DeckAction,
    apu::Channel,
    common::ResetKind,
    input::{JoypadBtn, Player},
    video::VideoFilter,
};
use tracing::warn;
use uuid::Uuid;
use winit::{
    event::{ElementState, MouseButton},
    keyboard::{KeyCode, ModifiersState},
};

macro_rules! action_binding {
    ($action:expr => $bindings:expr) => {{
        let action = $action.into();
        (action, ActionBindings::new(action, $bindings))
    }};
    ($action:expr => $modifiers:expr, $key:expr) => {
        action_binding!($action => [Some(Input::Key($key, $modifiers)), None, None])
    };
    ($action:expr => $modifiers1:expr, $key1:expr; $modifiers2:expr, $key2:expr) => {
        action_binding!(
            $action => [Some(Input::Key($key1, $modifiers1)), Some(Input::Key($key2, $modifiers2)), None]
        )
    };
}

#[allow(unused_macro_rules)]
macro_rules! shortcut_map {
    (@ $action:expr => $key:expr) => {
        action_binding!($action => ModifiersState::empty(), $key)
    };
    (@ $action:expr => $key1:expr; $key2:expr) => {
        action_binding!($action => ModifiersState::empty(), $key1; ModifiersState::empty(), $key2)
    };
    (@ $action:expr => :$modifiers:expr, $key:expr) => {
        action_binding!($action => $modifiers, $key)
    };
    (@ $action:expr => :$modifiers1:expr, $key1:expr; $key2:expr) => {
        action_binding!($action => $modifiers1, $key1; ModifiersState::empty(), $key2)
    };
    (@ $action:expr => :$modifiers1:expr, $key1:expr; :$modifiers2:expr, $key2:expr) => {
        action_binding!($action => $modifiers1, $key1; $modifiers2, $key2)
    };
    ($({ $action:expr => $(:$modifiers1:expr,) ?$key1:expr$(; $(:$modifiers2:expr,)? $key2:expr)? }),+$(,)?) => {
        vec![$(shortcut_map!(@ $action => $(:$modifiers1,)? $key1$(; $(:$modifiers2,)? $key2)?),)+]
    };
}

macro_rules! gamepad_map {
    (@ $action:expr => $player:expr; $button:expr) => {
        action_binding!($action => [Some(Input::Button($player, $button)), None, None])
    };
    (@ $action:expr => $player:expr; $button1:expr; ($button2:expr, $state:expr)) => {
        action_binding!($action => [Some(Input::Button($player, $button1)), Some(Input::Axis($player, $button2, $state)), None])
    };
    ($({ $action:expr => $player:expr; $button1:expr$(; ($button2:expr, $state:expr))? }),+$(,)?) => {
        vec![$(gamepad_map!(@ $action => $player; $button1$(; ($button2, $state))?),)+]
    };
}

macro_rules! mouse_map {
    (@ $action:expr => $button:expr) => {
        action_binding!($action => [Some(Input::Mouse($button)), None, None])
    };
    ($({ $action:expr => $button:expr }),+$(,)?) => {
        vec![$(mouse_map!(@ $action => $button),)+]
    };
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Input {
    Key(KeyCode, ModifiersState),
    Mouse(MouseButton),
    Button(Player, gilrs::Button),
    Axis(Player, gilrs::Axis, AxisDirection),
}

impl Input {
    pub fn fmt(input: Input) -> String {
        use winit::{
            event::MouseButton,
            keyboard::{KeyCode, ModifiersState},
        };

        match input {
            Input::Key(keycode, modifiers) => {
                let mut s = String::with_capacity(32);
                if modifiers.contains(ModifiersState::CONTROL) {
                    s += "Ctrl";
                }
                if modifiers.contains(ModifiersState::SHIFT) {
                    if !s.is_empty() {
                        s += "+";
                    }
                    s += "Shift";
                }
                if modifiers.contains(ModifiersState::ALT) {
                    if !s.is_empty() {
                        s += "+";
                    }
                    s += "Alt";
                }
                if modifiers.contains(ModifiersState::SUPER) {
                    if !s.is_empty() {
                        s += "+";
                    }
                    s += "Super";
                }
                let ch = match keycode {
                    KeyCode::Backquote => "`",
                    KeyCode::Backslash | KeyCode::IntlBackslash => "\\",
                    KeyCode::BracketLeft => "[",
                    KeyCode::BracketRight => "]",
                    KeyCode::Comma | KeyCode::NumpadComma => ",",
                    KeyCode::Digit0 => "0",
                    KeyCode::Digit1 => "1",
                    KeyCode::Digit2 => "2",
                    KeyCode::Digit3 => "3",
                    KeyCode::Digit4 => "4",
                    KeyCode::Digit5 => "5",
                    KeyCode::Digit6 => "6",
                    KeyCode::Digit7 => "7",
                    KeyCode::Digit8 => "8",
                    KeyCode::Digit9 => "9",
                    KeyCode::Equal => "=",
                    KeyCode::KeyA => "A",
                    KeyCode::KeyB => "B",
                    KeyCode::KeyC => "C",
                    KeyCode::KeyD => "D",
                    KeyCode::KeyE => "E",
                    KeyCode::KeyF => "F",
                    KeyCode::KeyG => "G",
                    KeyCode::KeyH => "H",
                    KeyCode::KeyI => "I",
                    KeyCode::KeyJ => "J",
                    KeyCode::KeyK => "K",
                    KeyCode::KeyL => "L",
                    KeyCode::KeyM => "M",
                    KeyCode::KeyN => "N",
                    KeyCode::KeyO => "O",
                    KeyCode::KeyP => "P",
                    KeyCode::KeyQ => "Q",
                    KeyCode::KeyR => "R",
                    KeyCode::KeyS => "S",
                    KeyCode::KeyT => "T",
                    KeyCode::KeyU => "U",
                    KeyCode::KeyV => "V",
                    KeyCode::KeyW => "W",
                    KeyCode::KeyX => "X",
                    KeyCode::KeyY => "Y",
                    KeyCode::KeyZ => "Z",
                    KeyCode::Minus | KeyCode::NumpadSubtract => "-",
                    KeyCode::Period | KeyCode::NumpadDecimal => ".",
                    KeyCode::Quote => "'",
                    KeyCode::Semicolon => ";",
                    KeyCode::Slash | KeyCode::NumpadDivide => "/",
                    KeyCode::Backspace | KeyCode::NumpadBackspace => "Backspace",
                    KeyCode::Enter | KeyCode::NumpadEnter => "Enter",
                    KeyCode::Space => "Space",
                    KeyCode::Tab => "Tab",
                    KeyCode::Delete => "Delete",
                    KeyCode::End => "End",
                    KeyCode::Help => "Help",
                    KeyCode::Home => "Home",
                    KeyCode::Insert => "Ins",
                    KeyCode::PageDown => "PageDown",
                    KeyCode::PageUp => "PageUp",
                    KeyCode::ArrowDown => "Down",
                    KeyCode::ArrowLeft => "Left",
                    KeyCode::ArrowRight => "Right",
                    KeyCode::ArrowUp => "Up",
                    KeyCode::Numpad0 => "Num0",
                    KeyCode::Numpad1 => "Num1",
                    KeyCode::Numpad2 => "Num2",
                    KeyCode::Numpad3 => "Num3",
                    KeyCode::Numpad4 => "Num4",
                    KeyCode::Numpad5 => "Num5",
                    KeyCode::Numpad6 => "Num6",
                    KeyCode::Numpad7 => "Num7",
                    KeyCode::Numpad8 => "Num8",
                    KeyCode::Numpad9 => "Num9",
                    KeyCode::NumpadAdd => "+",
                    KeyCode::NumpadEqual => "=",
                    KeyCode::NumpadHash => "#",
                    KeyCode::NumpadMultiply => "*",
                    KeyCode::NumpadParenLeft => "(",
                    KeyCode::NumpadParenRight => ")",
                    KeyCode::NumpadStar => "*",
                    KeyCode::Escape => "Escape",
                    KeyCode::Fn => "Fn",
                    KeyCode::F1 => "F1",
                    KeyCode::F2 => "F2",
                    KeyCode::F3 => "F3",
                    KeyCode::F4 => "F4",
                    KeyCode::F5 => "F5",
                    KeyCode::F6 => "F6",
                    KeyCode::F7 => "F7",
                    KeyCode::F8 => "F8",
                    KeyCode::F9 => "F9",
                    KeyCode::F10 => "F10",
                    KeyCode::F11 => "F11",
                    KeyCode::F12 => "F12",
                    KeyCode::F13 => "F13",
                    KeyCode::F14 => "F14",
                    KeyCode::F15 => "F15",
                    KeyCode::F16 => "F16",
                    KeyCode::F17 => "F17",
                    KeyCode::F18 => "F18",
                    KeyCode::F19 => "F19",
                    KeyCode::F20 => "F20",
                    KeyCode::F21 => "F21",
                    KeyCode::F22 => "F22",
                    KeyCode::F23 => "F23",
                    KeyCode::F24 => "F24",
                    KeyCode::F25 => "F25",
                    KeyCode::F26 => "F26",
                    KeyCode::F27 => "F27",
                    KeyCode::F28 => "F28",
                    KeyCode::F29 => "F29",
                    KeyCode::F30 => "F30",
                    KeyCode::F31 => "F31",
                    KeyCode::F32 => "F32",
                    KeyCode::F33 => "F33",
                    KeyCode::F34 => "F34",
                    KeyCode::F35 => "F35",
                    _ => "",
                };
                if !ch.is_empty() {
                    if !s.is_empty() {
                        s += "+";
                    }
                    s += ch;
                }
                s.shrink_to_fit();
                s
            }
            Input::Button(_, button) => format!("{button:#?}"),
            Input::Axis(_, axis, direction) => format!("{axis:#?} {direction:#?}"),
            Input::Mouse(button) => match button {
                MouseButton::Left => String::from("Left Click"),
                MouseButton::Right => String::from("Right Click"),
                MouseButton::Middle => String::from("Middle Click"),
                MouseButton::Back => String::from("Back Click"),
                MouseButton::Forward => String::from("Forward Click"),
                MouseButton::Other(id) => format!("Button {id} Click"),
            },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AxisDirection {
    Negative, // Left or Up
    Positive, // Right or Down
}

pub type Bindings = [Option<Input>; 3];

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub struct ActionBindings {
    pub action: Action,
    pub bindings: Bindings,
}

impl ActionBindings {
    pub const fn new(action: Action, bindings: Bindings) -> Self {
        Self { action, bindings }
    }

    pub fn empty(action: Action) -> Self {
        Self {
            action,
            bindings: Default::default(),
        }
    }
}

impl ActionBindings {
    pub fn default_shortcuts() -> BTreeMap<Action, ActionBindings> {
        use KeyCode::*;
        const SHIFT: ModifiersState = ModifiersState::SHIFT;
        const CONTROL: ModifiersState = ModifiersState::CONTROL;

        let mut bindings = Action::BINDABLE
            .into_iter()
            .filter(|action| !action.is_joypad())
            .map(|action| (action, ActionBindings::empty(action)))
            .collect::<BTreeMap<_, _>>();

        bindings.extend(shortcut_map!(
            { Debug::Step(DebugStep::Frame) => :SHIFT, KeyF },
            { Debug::Step(DebugStep::Into) => KeyC },
            { Debug::Step(DebugStep::Out) => :SHIFT, KeyO },
            { Debug::Step(DebugStep::Over) => KeyO },
            { Debug::Step(DebugStep::Scanline) => :SHIFT, KeyL },
            { Debug::Toggle(DebugKind::Apu) => :SHIFT, KeyA },
            { Debug::Toggle(DebugKind::Cpu) => :SHIFT, KeyD },
            { Debug::Toggle(DebugKind::Ppu) => :SHIFT, KeyP },
            { DeckAction::LoadState => :CONTROL, KeyL },
            { DeckAction::Reset(ResetKind::Hard) => :CONTROL, KeyH },
            { DeckAction::Reset(ResetKind::Soft) => :CONTROL, KeyR },
            { DeckAction::SaveState => :CONTROL, KeyS },
            { DeckAction::SetSaveSlot(1) => :CONTROL, Digit1 },
            { DeckAction::SetSaveSlot(2) => :CONTROL, Digit2 },
            { DeckAction::SetSaveSlot(3) => :CONTROL, Digit3 },
            { DeckAction::SetSaveSlot(4) => :CONTROL, Digit4 },
            { DeckAction::SetSaveSlot(5) => :CONTROL, Digit5 },
            { DeckAction::SetSaveSlot(6) => :CONTROL, Digit6 },
            { DeckAction::SetSaveSlot(7) => :CONTROL, Digit7 },
            { DeckAction::SetSaveSlot(8) => :CONTROL, Digit8 },
            { DeckAction::SetVideoFilter(VideoFilter::Ntsc) => :CONTROL, KeyN },
            { DeckAction::ToggleApuChannel(Channel::Dmc) => :SHIFT, Digit5 },
            { DeckAction::ToggleApuChannel(Channel::Mapper) => :SHIFT, Digit6 },
            { DeckAction::ToggleApuChannel(Channel::Noise) => :SHIFT, Digit4 },
            { DeckAction::ToggleApuChannel(Channel::Pulse1) => :SHIFT, Digit1 },
            { DeckAction::ToggleApuChannel(Channel::Pulse2) => :SHIFT, Digit2 },
            { DeckAction::ToggleApuChannel(Channel::Triangle) => :SHIFT, Digit3 },
            { Feature::InstantRewind => KeyR },
            { Feature::TakeScreenshot => F10 },
            { Feature::ToggleAudioRecording => :SHIFT, KeyR },
            { Feature::ToggleReplayRecording => :SHIFT, KeyV },
            { Feature::VisualRewind => KeyR },
            { Menu::About => F1 },
            { Menu::Keybinds => :CONTROL, KeyK; F3 },
            { Menu::Preferences => :CONTROL, KeyP; F2 },
            { Menu::PerfStats => :CONTROL, KeyF },
            { Setting::DecrementScale => :SHIFT, Minus },
            { Setting::DecrementSpeed => Minus },
            { Setting::FastForward => Space },
            { Setting::IncrementScale => :SHIFT, Equal },
            { Setting::IncrementSpeed => Equal },
            { Setting::ToggleAudio => :CONTROL, KeyM },
            { Setting::ToggleFullscreen => :CONTROL, Enter },
            { Setting::ToggleMenubar => :CONTROL, KeyE },
            { Ui::LoadRom => :CONTROL, KeyO; F3 },
            { Ui::Quit => :CONTROL, KeyQ },
            { Ui::TogglePause => Escape },
        ));
        bindings.extend(mouse_map!(
            { DeckAction::ZapperTrigger => MouseButton::Left },
            { DeckAction::ZapperAimOffscreen => MouseButton::Right }
        ));

        bindings
    }

    pub fn default_player_bindings(player: Player) -> BTreeMap<Action, ActionBindings> {
        use KeyCode::*;
        use gilrs::{Axis, Button};

        let mut bindings = Action::BINDABLE
            .into_iter()
            .filter(|action| action.joypad_player(player))
            .map(|action| (action, ActionBindings::empty(action)))
            .collect::<BTreeMap<_, _>>();

        bindings.extend(gamepad_map!(
            { (player, JoypadBtn::A) => player; Button::East },
            { (player, JoypadBtn::TurboA) => player; Button::North },
            { (player, JoypadBtn::B) => player; Button::South },
            { (player, JoypadBtn::TurboB) => player; Button::West },
            { (player, JoypadBtn::Up) => player; Button::DPadUp; (Axis::LeftStickY, AxisDirection::Negative) },
            { (player, JoypadBtn::Down) => player; Button::DPadDown; (Axis::LeftStickY, AxisDirection::Positive) },
            { (player, JoypadBtn::Left) => player; Button::DPadLeft; (Axis::LeftStickX, AxisDirection::Negative) },
            { (player, JoypadBtn::Right) => player; Button::DPadRight; (Axis::LeftStickX, AxisDirection::Positive) },
            { (player, JoypadBtn::Select) => player; Button::Select },
            { (player, JoypadBtn::Start) => player; Button::Start },
        ));

        let additional_bindings = match player {
            Player::One => shortcut_map!(
                { (Player::One, JoypadBtn::A) => KeyZ },
                { (Player::One, JoypadBtn::TurboA) => KeyA },
                { (Player::One, JoypadBtn::B) => KeyX },
                { (Player::One, JoypadBtn::TurboB) => KeyS },
                // FIXME: These overwrite Axis bindings above because there are only two binding
                // slots available at present
                { (Player::One, JoypadBtn::Up) => ArrowUp },
                { (Player::One, JoypadBtn::Down) => ArrowDown },
                { (Player::One, JoypadBtn::Left) => ArrowLeft },
                { (Player::One, JoypadBtn::Right) => ArrowRight },
                { (Player::One, JoypadBtn::Select) => KeyW },
                { (Player::One, JoypadBtn::Start) => KeyQ },
            ),
            _ => Vec::new(),
        };

        for (action, addtl_binding) in additional_bindings {
            if let Some((_, existing_bindings)) = bindings
                .iter_mut()
                .find(|(existing_action, _)| **existing_action == action)
            {
                for binding in &mut existing_bindings.bindings {
                    if binding.is_none() {
                        *binding = addtl_binding.bindings[0];
                    }
                }
            } else {
                bindings.insert(action, addtl_binding);
            }
        }

        bindings
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputBindings(HashMap<Input, Action>);

impl InputBindings {
    pub fn from_input_config(cfg: &InputConfig) -> Self {
        Self(
            cfg.action_bindings
                .iter()
                .flat_map(|bind| {
                    bind.bindings
                        .iter()
                        .flatten()
                        .map(|input| (*input, bind.action))
                })
                .collect(),
        )
    }
}

impl Deref for InputBindings {
    type Target = HashMap<Input, Action>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for InputBindings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Represents gamepad input state.
#[derive(Default, Debug)]
pub struct Gamepads {
    connected: HashMap<gilrs::GamepadId, Uuid>,
    inner: Option<gilrs::Gilrs>,
    events: VecDeque<gilrs::Event>,
}

impl Gamepads {
    pub fn new() -> Self {
        let mut connected = HashMap::default();
        let mut gilrs = gilrs::Gilrs::new();
        let mut events = VecDeque::new();
        match &mut gilrs {
            Ok(inputs) => {
                for (id, gamepad) in inputs.gamepads() {
                    let uuid = Self::create_uuid(&gamepad);
                    tracing::debug!("gamepad connected: {} ({uuid})", gamepad.name());
                    connected.insert(id, uuid);
                }
                events.reserve(256);
            }
            Err(err) => {
                warn!("failed to initialize inputs: {err:?}");
            }
        }

        Self {
            connected,
            inner: gilrs.ok(),
            events,
        }
    }

    pub fn update_events(&mut self) {
        if let Some(inner) = self.inner.as_mut() {
            while let Some(event) = inner.next_event() {
                self.events.push_back(event);
            }
        }
    }

    pub fn axis_state(value: f32) -> (Option<AxisDirection>, ElementState) {
        let direction = if value >= 0.6 {
            Some(AxisDirection::Positive)
        } else if value <= -0.6 {
            Some(AxisDirection::Negative)
        } else {
            None
        };
        let state = if direction.is_some() {
            ElementState::Pressed
        } else {
            ElementState::Released
        };
        (direction, state)
    }

    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }

    pub fn input_from_event(
        &self,
        event: &gilrs::Event,
        cfg: &Config,
    ) -> Option<(Input, ElementState)> {
        use gilrs::EventType;
        if let Some(player) = self
            .connected
            .get(&event.id)
            .and_then(|uuid| cfg.input.gamepad_assignment(uuid))
        {
            match event.event {
                EventType::ButtonPressed(button, _) => {
                    Some((Input::Button(player, button), ElementState::Pressed))
                }
                EventType::ButtonRepeated(button, _) => {
                    Some((Input::Button(player, button), ElementState::Pressed))
                }
                EventType::ButtonReleased(button, _) => {
                    Some((Input::Button(player, button), ElementState::Released))
                }
                EventType::AxisChanged(axis, value, _) => {
                    if let (Some(direction), state) = Gamepads::axis_state(value) {
                        Some((Input::Axis(player, axis, direction), state))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn connected_gamepad(&self, id: gilrs::GamepadId) -> Option<gilrs::Gamepad<'_>> {
        self.inner
            .as_ref()
            .and_then(|inner| inner.connected_gamepad(id))
    }

    pub fn gamepad(&self, id: gilrs::GamepadId) -> Option<gilrs::Gamepad<'_>> {
        self.inner.as_ref().map(|inner| inner.gamepad(id))
    }

    pub fn gamepad_by_uuid(&self, uuid: &Uuid) -> Option<gilrs::Gamepad<'_>> {
        self.inner.as_ref().and_then(|inner| {
            self.connected
                .iter()
                .find(|(_, u)| *u == uuid)
                .and_then(|(id, _)| inner.connected_gamepad(*id))
        })
    }

    pub fn gamepad_name_by_uuid(&self, uuid: &Uuid) -> Option<String> {
        self.gamepad_by_uuid(uuid).map(|g| g.name().to_string())
    }

    pub fn gamepad_uuid(&self, id: gilrs::GamepadId) -> Option<Uuid> {
        self.connected_gamepad(id).map(|g| Self::create_uuid(&g))
    }

    pub fn is_connected(&self, uuid: &Uuid) -> bool {
        self.gamepad_by_uuid(uuid).is_some()
    }

    pub fn list(&self) -> Option<Peekable<gilrs::ConnectedGamepadsIterator<'_>>> {
        self.inner.as_ref().map(|inner| inner.gamepads().peekable())
    }

    pub fn connected_uuids(&self) -> impl Iterator<Item = &Uuid> {
        self.connected.values()
    }

    pub fn events(&self) -> impl Iterator<Item = &gilrs::Event> {
        self.events.iter()
    }

    pub fn next_event(&mut self) -> Option<gilrs::Event> {
        self.events.pop_back()
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    pub fn connect(&mut self, gamepad_id: gilrs::GamepadId) {
        if let Some(gamepad) = self.connected_gamepad(gamepad_id) {
            let uuid = Self::create_uuid(&gamepad);
            tracing::debug!("gamepad connected: {} ({uuid})", gamepad.name());
            self.connected.insert(gamepad.id(), uuid);
        }
    }

    pub fn disconnect(&mut self, gamepad_id: gilrs::GamepadId) {
        if let Some(gamepad) = self.gamepad(gamepad_id) {
            let uuid = Self::create_uuid(&gamepad);
            tracing::debug!("gamepad disconnected: {} ({uuid})", gamepad.name());
        }
        self.connected.remove(&gamepad_id);
    }

    pub fn create_uuid(gamepad: &gilrs::Gamepad<'_>) -> Uuid {
        let uuid = Uuid::from_bytes(gamepad.uuid());
        if uuid != Uuid::nil() {
            return uuid;
        }

        // See: https://gitlab.com/gilrs-project/gilrs/-/issues/107

        // SDL always uses USB bus for UUID
        let bustype = u32::to_be(0x03);

        // Version is not available.
        let version = 0;
        let vendor_id = gamepad.vendor_id().unwrap_or(0);
        let product_id = gamepad.product_id().unwrap_or(0);

        if vendor_id == 0 && product_id == 0 {
            Uuid::new_v4()
        } else {
            Uuid::from_fields(
                bustype,
                vendor_id,
                0,
                &[
                    (product_id >> 8) as u8,
                    product_id as u8,
                    0,
                    0,
                    (version >> 8) as u8,
                    version as u8,
                    0,
                    0,
                ],
            )
        }
    }
}
