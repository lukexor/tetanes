use crate::nes::renderer::gui::Menu;
use serde::{Deserialize, Serialize};
use tetanes_core::{
    apu::Channel, common::NesRegion, input::JoypadBtn, mapper::MapperRevision, video::VideoFilter,
};

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Ui(UiState),
    Menu(Menu),
    Feature(Feature),
    Setting(Setting),
    Joypad(JoypadBtn),
    ZapperTrigger,
    Debug(DebugAction),
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
        Self::Joypad(btn)
    }
}

impl From<DebugAction> for Action {
    fn from(action: DebugAction) -> Self {
        Self::Debug(action)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiState {
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
    SetNesRegion(NesRegion),
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
pub enum DebugStep {
    Into,
    Out,
    Over,
    Scanline,
    Frame,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum DebugAction {
    ToggleDebugger(Debugger),
    Step(DebugStep),
    UpdateScanline(isize),
}
