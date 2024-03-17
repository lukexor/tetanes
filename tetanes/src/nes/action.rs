use crate::nes::renderer::gui::Menu;
use serde::{Deserialize, Serialize};
use tetanes_core::{action::Action as DeckAction, input::JoypadBtn};

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Ui(UiState),
    Menu(Menu),
    Feature(Feature),
    Setting(Setting),
    Deck(DeckAction),
    Debug(Debugger),
}

impl From<UiState> for Action {
    fn from(state: UiState) -> Self {
        Self::Ui(state)
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
        Self::Deck(DeckAction::Joypad(btn))
    }
}

impl From<DeckAction> for Action {
    fn from(deck: DeckAction) -> Self {
        Self::Deck(deck)
    }
}

impl From<Debugger> for Action {
    fn from(action: Debugger) -> Self {
        Self::Debug(action)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiState {
    Quit,
    TogglePause,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Feature {
    ToggleReplayRecord,
    ToggleAudioRecord,
    Rewind,
    TakeScreenshot,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Setting {
    ToggleFullscreen,
    ToggleVsync,
    ToggleAudio,
    FastForward,
    IncSpeed,
    DecSpeed,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum DebugKind {
    Cpu,
    Ppu,
    Apu,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum DebugStep {
    Into,
    Out,
    Over,
    Scanline,
    Frame,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Debugger {
    ToggleDebugger(DebugKind),
    Step(DebugStep),
    UpdateScanline(isize),
}
