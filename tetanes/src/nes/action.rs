//! An [`Action`] is an enumerated list of possible state changes to the `TetaNES` application that
//! allows for event handling and test abstractions such as being able to map a custom keybind to a
//! given state change.

use crate::nes::renderer::gui::Menu;
use serde::{Deserialize, Serialize};
use tetanes_core::{
    action::Action as DeckAction,
    apu::Channel,
    common::{NesRegion, ResetKind},
    input::{FourPlayer, JoypadBtn, Player},
    mapper::{Bf909Revision, MapperRevision, Mmc3Revision},
    video::VideoFilter,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Ui(Ui),
    Menu(Menu),
    Feature(Feature),
    Setting(Setting),
    Deck(DeckAction),
    Debug(Debug),
}

impl Action {
    pub const BINDABLE: [Self; 109] = [
        Self::Ui(Ui::Quit),
        Self::Ui(Ui::TogglePause),
        Self::Ui(Ui::LoadRom),
        Self::Ui(Ui::UnloadRom),
        Self::Ui(Ui::LoadReplay),
        Self::Menu(Menu::About),
        Self::Menu(Menu::Keybinds),
        Self::Menu(Menu::PerfStats),
        Self::Menu(Menu::Preferences),
        Self::Feature(Feature::ToggleReplayRecording),
        Self::Feature(Feature::ToggleAudioRecording),
        Self::Feature(Feature::VisualRewind),
        Self::Feature(Feature::InstantRewind),
        Self::Feature(Feature::TakeScreenshot),
        Self::Setting(Setting::ToggleFullscreen),
        Self::Setting(Setting::ToggleAudio),
        Self::Setting(Setting::ToggleCycleAccurate),
        Self::Setting(Setting::ToggleRewinding),
        Self::Setting(Setting::ToggleOverscan),
        Self::Setting(Setting::ToggleMenubar),
        Self::Setting(Setting::ToggleMessages),
        Self::Setting(Setting::ToggleFps),
        Self::Setting(Setting::FastForward),
        Self::Setting(Setting::IncrementScale),
        Self::Setting(Setting::DecrementScale),
        Self::Setting(Setting::IncrementSpeed),
        Self::Setting(Setting::DecrementSpeed),
        Self::Deck(DeckAction::Reset(ResetKind::Soft)),
        Self::Deck(DeckAction::Reset(ResetKind::Hard)),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::Left))),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::Right))),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::Up))),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::Down))),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::A))),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::B))),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::TurboA))),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::TurboB))),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::Select))),
        Self::Deck(DeckAction::Joypad((Player::One, JoypadBtn::Start))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::Left))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::Right))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::Up))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::Down))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::A))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::B))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::TurboA))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::TurboB))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::Select))),
        Self::Deck(DeckAction::Joypad((Player::Two, JoypadBtn::Start))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::Left))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::Right))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::Up))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::Down))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::A))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::B))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::TurboA))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::TurboB))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::Select))),
        Self::Deck(DeckAction::Joypad((Player::Three, JoypadBtn::Start))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::Left))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::Right))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::Up))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::Down))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::A))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::B))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::TurboA))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::TurboB))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::Select))),
        Self::Deck(DeckAction::Joypad((Player::Four, JoypadBtn::Start))),
        Self::Deck(DeckAction::ToggleZapperConnected),
        // Self::Deck(DeckAction::ZapperAim), // Binding doesn't make sense
        Self::Deck(DeckAction::ZapperTrigger),
        Self::Deck(DeckAction::FourPlayer(FourPlayer::Disabled)),
        Self::Deck(DeckAction::FourPlayer(FourPlayer::FourScore)),
        Self::Deck(DeckAction::FourPlayer(FourPlayer::Satellite)),
        // Only allow bindings up to 8 slots
        Self::Deck(DeckAction::SetSaveSlot(1)),
        Self::Deck(DeckAction::SetSaveSlot(2)),
        Self::Deck(DeckAction::SetSaveSlot(3)),
        Self::Deck(DeckAction::SetSaveSlot(4)),
        Self::Deck(DeckAction::SetSaveSlot(5)),
        Self::Deck(DeckAction::SetSaveSlot(6)),
        Self::Deck(DeckAction::SetSaveSlot(7)),
        Self::Deck(DeckAction::SetSaveSlot(8)),
        Self::Deck(DeckAction::SaveState),
        Self::Deck(DeckAction::LoadState),
        Self::Deck(DeckAction::ToggleApuChannel(Channel::Pulse1)),
        Self::Deck(DeckAction::ToggleApuChannel(Channel::Pulse2)),
        Self::Deck(DeckAction::ToggleApuChannel(Channel::Triangle)),
        Self::Deck(DeckAction::ToggleApuChannel(Channel::Noise)),
        Self::Deck(DeckAction::ToggleApuChannel(Channel::Dmc)),
        Self::Deck(DeckAction::ToggleApuChannel(Channel::Mapper)),
        Self::Deck(DeckAction::MapperRevision(MapperRevision::Mmc3(
            Mmc3Revision::A,
        ))),
        Self::Deck(DeckAction::MapperRevision(MapperRevision::Mmc3(
            Mmc3Revision::BC,
        ))),
        Self::Deck(DeckAction::MapperRevision(MapperRevision::Mmc3(
            Mmc3Revision::Acc,
        ))),
        Self::Deck(DeckAction::MapperRevision(MapperRevision::Bf909(
            Bf909Revision::Bf909x,
        ))),
        Self::Deck(DeckAction::MapperRevision(MapperRevision::Bf909(
            Bf909Revision::Bf9097,
        ))),
        Self::Deck(DeckAction::SetNesRegion(NesRegion::Auto)),
        Self::Deck(DeckAction::SetNesRegion(NesRegion::Ntsc)),
        Self::Deck(DeckAction::SetNesRegion(NesRegion::Pal)),
        Self::Deck(DeckAction::SetNesRegion(NesRegion::Dendy)),
        Self::Deck(DeckAction::SetVideoFilter(VideoFilter::Pixellate)),
        Self::Deck(DeckAction::SetVideoFilter(VideoFilter::Ntsc)),
        Self::Debug(Debug::Toggle(Debugger::Cpu)),
        Self::Debug(Debug::Toggle(Debugger::Ppu)),
        Self::Debug(Debug::Toggle(Debugger::Apu)),
        Self::Debug(Debug::Step(DebugStep::Into)),
        Self::Debug(Debug::Step(DebugStep::Out)),
        Self::Debug(Debug::Step(DebugStep::Over)),
        Self::Debug(Debug::Step(DebugStep::Scanline)),
        Self::Debug(Debug::Step(DebugStep::Frame)),
    ];

    pub const fn is_joypad(&self) -> bool {
        matches!(self, Action::Deck(DeckAction::Joypad(_)))
    }
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl AsRef<str> for Action {
    fn as_ref(&self) -> &str {
        match self {
            Action::Ui(ui) => match ui {
                Ui::Quit => "Quit",
                Ui::TogglePause => "Toggle Pause",
                Ui::LoadRom => "Load ROM",
                Ui::UnloadRom => "Unload ROM",
                Ui::LoadReplay => "Load Replay",
            },
            Action::Menu(menu) => match menu {
                Menu::About => "Toggle About Window",
                Menu::Keybinds => "Toggle Keybinds Window",
                Menu::PerfStats => "Toggle Performance Stats Window",
                Menu::Preferences => "Toggle Preferences Window",
            },
            Action::Feature(feature) => match feature {
                Feature::ToggleReplayRecording => "Toggle Replay Recording",
                Feature::ToggleAudioRecording => "Toggle Audio Recording",
                Feature::VisualRewind => "Visual Rewind",
                Feature::InstantRewind => "Instant Rewind",
                Feature::TakeScreenshot => "Take Screenshot",
            },
            Action::Setting(setting) => match setting {
                Setting::ToggleFullscreen => "Toggle Fullscreen",
                Setting::ToggleAudio => "Toggle Audio",
                Setting::ToggleCycleAccurate => "Toggle Cycle Accurate",
                Setting::ToggleRewinding => "Toggle Rewinding",
                Setting::ToggleOverscan => "Toggle Overscan",
                Setting::ToggleMenubar => "Toggle Menubar",
                Setting::ToggleMessages => "Toggle Messages",
                Setting::ToggleFps => "Toggle FPS",
                Setting::FastForward => "Fast Forward",
                Setting::IncrementScale => "Increment Scale",
                Setting::DecrementScale => "Decrement Scale",
                Setting::IncrementSpeed => "Increment Speed",
                Setting::DecrementSpeed => "Decrement Speed",
            },
            Action::Deck(deck) => match deck {
                DeckAction::Reset(kind) => match kind {
                    ResetKind::Soft => "Reset",
                    ResetKind::Hard => "Power Cycle",
                },
                DeckAction::Joypad((_, joypad)) => match joypad {
                    JoypadBtn::Left => "Joypad Left",
                    JoypadBtn::Right => "Joypad Right",
                    JoypadBtn::Up => "Joypad Up",
                    JoypadBtn::Down => "Joypad Down",
                    JoypadBtn::A => "Joypad A",
                    JoypadBtn::B => "Joypad B",
                    JoypadBtn::TurboA => "Joypad Turbo A",
                    JoypadBtn::TurboB => "Joypad Turbo B",
                    JoypadBtn::Select => "Joypad Select",
                    JoypadBtn::Start => "Joypad Start",
                },
                DeckAction::ToggleZapperConnected => "Toggle Zapper Connected",
                DeckAction::ZapperAim(_) => "Zapper Aim",
                DeckAction::ZapperAimOffscreen => "Zapper Aim Offscreen (Hold)",
                DeckAction::ZapperTrigger => "Zapper Trigger",
                DeckAction::FourPlayer(FourPlayer::Disabled) => "Disable Four Player Mode",
                DeckAction::FourPlayer(FourPlayer::FourScore) => "Enable Four Player (FourScore)",
                DeckAction::FourPlayer(FourPlayer::Satellite) => "Enable Four Player (Satellite)",
                DeckAction::SetSaveSlot(1) => "Set Save Slot 1",
                DeckAction::SetSaveSlot(2) => "Set Save Slot 2",
                DeckAction::SetSaveSlot(3) => "Set Save Slot 3",
                DeckAction::SetSaveSlot(4) => "Set Save Slot 4",
                DeckAction::SetSaveSlot(5) => "Set Save Slot 5",
                DeckAction::SetSaveSlot(6) => "Set Save Slot 6",
                DeckAction::SetSaveSlot(7) => "Set Save Slot 7",
                DeckAction::SetSaveSlot(8) => "Set Save Slot 8",
                DeckAction::SetSaveSlot(_) => "Set Save Slot N",
                DeckAction::SaveState => "Save State",
                DeckAction::LoadState => "Load State",
                DeckAction::ToggleApuChannel(channel) => match channel {
                    Channel::Pulse1 => "Toggle Pulse1 Channel",
                    Channel::Pulse2 => "Toggle Pulse2 Channel",
                    Channel::Triangle => "Toggle Triangle Channel",
                    Channel::Noise => "Toggle Noise Channel",
                    Channel::Dmc => "Toggle DMC Channel",
                    Channel::Mapper => "Toggle Mapper Channel",
                },
                DeckAction::MapperRevision(rev) => match rev {
                    MapperRevision::Mmc3(mmc3) => match mmc3 {
                        Mmc3Revision::A => "Set Mapper Rev. to MMC3A",
                        Mmc3Revision::BC => "Set Mapper Rev. to MMC3B/C",
                        Mmc3Revision::Acc => "Set Mapper Rev. to MC-ACC",
                    },
                    MapperRevision::Bf909(bf909) => match bf909 {
                        Bf909Revision::Bf909x => "Set Mapper Rev. to BF909x",
                        Bf909Revision::Bf9097 => "Set Mapper Rev. to BF9097",
                    },
                },
                DeckAction::SetNesRegion(region) => match region {
                    NesRegion::Auto => "Set Region to Auto-Detect",
                    NesRegion::Ntsc => "Set Region to NTSC",
                    NesRegion::Pal => "Set Region to PAL",
                    NesRegion::Dendy => "Set Region to Dendy",
                },
                DeckAction::SetVideoFilter(filter) => match filter {
                    VideoFilter::Pixellate => "Set Filter to Pixellate",
                    VideoFilter::Ntsc => "Set Filter to NTSC",
                },
            },
            Action::Debug(debug) => match debug {
                Debug::Toggle(debugger) => match debugger {
                    Debugger::Cpu => "Toggle CPU Debugger",
                    Debugger::Ppu => "Toggle PPU Debugger",
                    Debugger::Apu => "Toggle APU Debugger",
                },
                Debug::Step(step) => match step {
                    DebugStep::Into => "Step Into (CPU Debugger)",
                    DebugStep::Out => "Step Out (CPU Debugger)",
                    DebugStep::Over => "Step Over (CPU Debugger)",
                    DebugStep::Scanline => "Step Scanline (CPU Debugger)",
                    DebugStep::Frame => "Step Frame (CPU Debugger)",
                },
            },
        }
    }
}

impl From<Ui> for Action {
    fn from(state: Ui) -> Self {
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

impl From<(Player, JoypadBtn)> for Action {
    fn from((player, btn): (Player, JoypadBtn)) -> Self {
        Self::Deck(DeckAction::Joypad((player, btn)))
    }
}

impl From<DeckAction> for Action {
    fn from(deck: DeckAction) -> Self {
        Self::Deck(deck)
    }
}

impl From<Debug> for Action {
    fn from(action: Debug) -> Self {
        Self::Debug(action)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ui {
    Quit,
    TogglePause,
    LoadRom,
    LoadReplay,
    UnloadRom,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Feature {
    ToggleReplayRecording,
    ToggleAudioRecording,
    VisualRewind,
    InstantRewind,
    TakeScreenshot,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Setting {
    ToggleFullscreen,
    ToggleAudio,
    ToggleCycleAccurate,
    ToggleRewinding,
    ToggleOverscan,
    ToggleMenubar,
    ToggleMessages,
    ToggleFps,
    FastForward,
    IncrementScale,
    DecrementScale,
    IncrementSpeed,
    DecrementSpeed,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Debugger {
    Cpu,
    Ppu,
    Apu,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum DebugStep {
    Into,
    Out,
    Over,
    Scanline,
    Frame,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Debug {
    Toggle(Debugger),
    Step(DebugStep),
}
