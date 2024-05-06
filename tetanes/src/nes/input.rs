use crate::nes::{
    action::{Action, Debug, DebugStep, Debugger, Feature, Setting, Ui},
    config::InputConfig,
    renderer::gui::Menu,
};
use egui::ahash::{HashMap, HashMapExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
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
use winit::{
    event::{ElementState, MouseButton},
    keyboard::{KeyCode, ModifiersState},
};

macro_rules! action_binding {
    ($action:expr => $bindings:expr) => {
        ActionBindings {
            action: $action.into(),
            bindings: $bindings,
        }
    };
    ($action:expr => $modifiers:expr, $key:expr) => {
        action_binding!($action => [Some(Input::Key($key, $modifiers)), None])
    };
    ($action:expr => $modifiers1:expr, $key1:expr; $modifiers2:expr, $key2:expr) => {
        action_binding!(
            $action => [Some(Input::Key($key1, $modifiers1)), Some(Input::Key($key2, $modifiers2))]
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
        action_binding!($action => [Some(Input::Button($player, $button)), None])
    };
    ($({ $action:expr => $player:expr; $button:expr }),+$(,)?) => {
        vec![$(gamepad_map!(@ $action => $player; $button),)+]
    };
}

macro_rules! mouse_map {
    (@ $action:expr => $button:expr) => {
        action_binding!($action => [Some(Input::Mouse($button)), None])
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
    Axis(Player, gilrs::Axis),
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub struct ActionBindings {
    pub action: Action,
    pub bindings: [Option<Input>; 2],
}

impl ActionBindings {
    pub fn empty(action: Action) -> Self {
        Self {
            action,
            bindings: Default::default(),
        }
    }

    pub fn default_shortcuts() -> Vec<Self> {
        use KeyCode::*;
        const SHIFT: ModifiersState = ModifiersState::SHIFT;
        const CONTROL: ModifiersState = ModifiersState::CONTROL;

        let mut bindings = Vec::with_capacity(64);
        bindings.extend(shortcut_map!(
            { Debug::Step(DebugStep::Frame) => :SHIFT, KeyF },
            { Debug::Step(DebugStep::Into) => KeyC },
            { Debug::Step(DebugStep::Out) => :SHIFT, KeyO },
            { Debug::Step(DebugStep::Over) => KeyO },
            { Debug::Step(DebugStep::Scanline) => :SHIFT, KeyL },
            { Debug::Toggle(Debugger::Apu) => :SHIFT, KeyA },
            { Debug::Toggle(Debugger::Cpu) => :SHIFT, KeyD },
            { Debug::Toggle(Debugger::Ppu) => :SHIFT, KeyP },
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
            { Menu::Keybinds => :CONTROL, KeyK },
            { Menu::Preferences => :CONTROL, KeyP; F2 },
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
        bindings.shrink_to_fit();

        bindings
    }

    pub fn default_player_bindings(player: Player) -> Vec<Self> {
        use gilrs::Button;
        use KeyCode::*;

        let mut bindings = Vec::with_capacity(10);
        bindings.extend(gamepad_map!(
            { (player, JoypadBtn::A) => player; Button::East },
            { (player, JoypadBtn::TurboA) => player; Button::North },
            { (player, JoypadBtn::B) => player; Button::South },
            { (player, JoypadBtn::TurboB) => player; Button::West },
            { (player, JoypadBtn::Up) => player; Button::DPadUp },
            { (player, JoypadBtn::Down) => player; Button::DPadDown },
            { (player, JoypadBtn::Left) => player; Button::DPadLeft },
            { (player, JoypadBtn::Right) => player; Button::DPadRight },
            { (player, JoypadBtn::Select) => player; Button::Select },
            { (player, JoypadBtn::Start) => player; Button::Start },
        ));
        let additional_bindings = match player {
            Player::One => shortcut_map!(
                { (Player::One, JoypadBtn::A) => KeyZ },
                { (Player::One, JoypadBtn::TurboA) => KeyA },
                { (Player::One, JoypadBtn::B) => KeyX },
                { (Player::One, JoypadBtn::TurboB) => KeyS },
                { (Player::One, JoypadBtn::Up) => ArrowUp },
                { (Player::One, JoypadBtn::Down) => ArrowDown },
                { (Player::One, JoypadBtn::Left) => ArrowLeft },
                { (Player::One, JoypadBtn::Right) => ArrowRight },
                { (Player::One, JoypadBtn::Select) => KeyW },
                { (Player::One, JoypadBtn::Start) => KeyQ },
            ),
            Player::Two => shortcut_map!(
                { (Player::Two, JoypadBtn::A) => KeyN },
                { (Player::Two, JoypadBtn::B) => KeyM },
                { (Player::Two, JoypadBtn::Up) => KeyI },
                { (Player::Two, JoypadBtn::Down) => KeyK },
                { (Player::Two, JoypadBtn::Left) => KeyJ },
                { (Player::Two, JoypadBtn::Right) => KeyL },
                { (Player::Two, JoypadBtn::Select) => Digit9 },
                { (Player::Two, JoypadBtn::Start) => Digit8 },
            ),
            #[cfg(debug_assertions)]
            Player::Three => shortcut_map!(
                { (Player::Three, JoypadBtn::A) => KeyV },
                { (Player::Three, JoypadBtn::B) => KeyB },
                { (Player::Three, JoypadBtn::Up) => KeyT },
                { (Player::Three, JoypadBtn::Down) => KeyG },
                { (Player::Three, JoypadBtn::Left) => KeyF },
                { (Player::Three, JoypadBtn::Right) => KeyH },
                { (Player::Three, JoypadBtn::Select) => Digit6 },
                { (Player::Three, JoypadBtn::Start) => Digit5 },
            ),
            Player::Four => Vec::new(),
        };
        for binding in additional_bindings {
            if let Some(existing_bind) = bindings.iter_mut().find(|b| b.action == binding.action) {
                if existing_bind.bindings[0].is_some() {
                    existing_bind.bindings[1] = binding.bindings[0];
                }
            } else {
                bindings.push(binding);
            }
        }

        bindings
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputBindings(HashMap<Input, Action>);

impl InputBindings {
    pub fn from_input_config(config: &InputConfig) -> Self {
        let mut map = HashMap::with_capacity(256);
        for bind in config
            .shortcuts
            .iter()
            .chain(config.joypad_bindings.iter().flatten())
        {
            for input in bind.bindings.into_iter().flatten() {
                map.insert(input, bind.action);
            }
        }
        map.shrink_to_fit();
        Self(map)
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
    connected: [Option<gilrs::GamepadId>; 4],
    inner: Option<gilrs::Gilrs>,
    events: VecDeque<gilrs::Event>,
}

impl Gamepads {
    pub fn new() -> Self {
        let mut connected = [None; 4];
        let mut gilrs = gilrs::Gilrs::new();
        let mut events = VecDeque::new();
        match &mut gilrs {
            Ok(inputs) => {
                for (gamepad, (id, _)) in connected.iter_mut().zip(inputs.gamepads()) {
                    *gamepad = Some(id);
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

    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }

    pub fn input_from_event(&self, event: &gilrs::Event) -> Option<(Input, ElementState)> {
        use gilrs::EventType;
        if let Some(player) = self.player(event.id) {
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
                EventType::ButtonChanged(_, _, _) => None,
                EventType::AxisChanged(_, _, _) => None,
                EventType::Connected | EventType::Disconnected | EventType::Dropped => None,
            }
        } else {
            None
        }
    }

    pub fn gamepad(&self, player: Player) -> Option<gilrs::Gamepad<'_>> {
        self.inner.as_ref().and_then(|inner| {
            self.connected[player as usize].and_then(|id| inner.connected_gamepad(id))
        })
    }

    pub fn list(&self) -> Option<Peekable<gilrs::ConnectedGamepadsIterator<'_>>> {
        self.inner.as_ref().map(|inner| inner.gamepads().peekable())
    }

    pub fn player(&self, gamepad_id: gilrs::GamepadId) -> Option<Player> {
        self.connected
            .iter()
            .position(|g| g.is_some_and(|g| g == gamepad_id))
            .and_then(|i| Player::try_from(i).ok())
    }

    pub fn next_unassigned(&mut self) -> Option<Player> {
        self.connected
            .iter()
            .position(|g| g.is_none())
            .and_then(|i| Player::try_from(i).ok())
    }

    pub fn next_event(&mut self) -> Option<gilrs::Event> {
        self.events.pop_back()
    }

    pub fn assign(&mut self, player: Player, gamepad_id: gilrs::GamepadId) {
        self.connected[player as usize] = Some(gamepad_id);
    }

    pub fn unassign(&mut self, player: Player) {
        self.connected[player as usize] = None;
    }

    pub fn disconnect(&mut self, gamepad_id: gilrs::GamepadId) {
        if let Some(gamepad) = self
            .connected
            .iter_mut()
            .find(|g| g.is_some_and(|g| g == gamepad_id))
        {
            *gamepad = None;
        }
    }
}
