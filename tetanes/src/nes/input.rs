use crate::nes::{
    action::{Action, DebugKind, DebugStep, Debugger, Feature, Setting, UiState},
    renderer::gui::{ConfigTab, Menu},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};
use tetanes_core::{
    action::Action as DeckAction,
    apu::Channel,
    input::{JoypadBtn, Player},
    video::VideoFilter,
};
use winit::{
    event::{ElementState, MouseButton},
    keyboard::{KeyCode, ModifiersState},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Config {
    // Since Input is a compound key and not serializable, input_bindings is derived from input_map
    #[serde(skip)]
    pub input_map: InputMap,
    pub input_bindings: Vec<InputBinding>,
    pub controller_deadzone: f64,
}

impl Default for Config {
    fn default() -> Self {
        let input_map = InputMap::default();
        let input_bindings = input_map
            .iter()
            .map(|(input, (slot, action))| (*input, *slot, *action))
            .collect();
        Self {
            input_map,
            input_bindings,
            controller_deadzone: 0.5,
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
        key_map!(map, One, ShiftRight, SHIFT, JoypadBtn::Select); // Required because shift is also a modifier
        key_map!(map, Two, KeyJ, JoypadBtn::Left);
        key_map!(map, Two, KeyL, JoypadBtn::Right);
        key_map!(map, Two, KeyI, JoypadBtn::Up);
        key_map!(map, Two, KeyK, JoypadBtn::Down);
        key_map!(map, Two, KeyN, JoypadBtn::A);
        key_map!(map, Two, KeyM, JoypadBtn::B);
        key_map!(map, Two, Numpad8, JoypadBtn::Start);
        key_map!(map, Two, Numpad9, SHIFT, JoypadBtn::Select);
        #[cfg(debug_assertions)]
        {
            key_map!(map, Three, KeyF, JoypadBtn::Left);
            key_map!(map, Three, KeyH, JoypadBtn::Right);
            key_map!(map, Three, KeyT, JoypadBtn::Up);
            key_map!(map, Three, KeyG, JoypadBtn::Down);
            key_map!(map, Three, KeyV, JoypadBtn::A);
            key_map!(map, Three, KeyB, JoypadBtn::B);
            key_map!(map, Three, Numpad5, JoypadBtn::Start);
            key_map!(map, Three, Numpad6, SHIFT, JoypadBtn::Select);
        }
        key_map!(map, One, Escape, UiState::TogglePause);
        key_map!(map, One, KeyH, CONTROL, Menu::About);
        key_map!(map, One, F1, Menu::About);
        key_map!(map, One, KeyC, CONTROL, Menu::Config(ConfigTab::General));
        key_map!(map, One, F2, Menu::Config(ConfigTab::General));
        key_map!(map, One, KeyO, CONTROL, UiState::LoadRom);
        key_map!(map, One, F3, UiState::LoadRom);
        key_map!(map, One, KeyK, CONTROL, Menu::Keybind(Player::One));
        key_map!(map, One, KeyQ, CONTROL, UiState::Quit);
        key_map!(map, One, KeyR, CONTROL, DeckAction::SoftReset);
        key_map!(map, One, KeyP, CONTROL, DeckAction::HardReset);
        key_map!(map, One, Equal, Setting::IncSpeed);
        key_map!(map, One, Minus, Setting::DecSpeed);
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
        key_map!(map, One, KeyE, CONTROL, Setting::ToggleMenuBar);
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
        key_map!(
            map,
            One,
            Digit6,
            SHIFT,
            DeckAction::ToggleApuChannel(Channel::Mapper)
        );
        key_map!(map, One, Enter, CONTROL, Setting::ToggleFullscreen);
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
