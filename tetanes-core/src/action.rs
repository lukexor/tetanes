use crate::{
    apu::Channel,
    common::{NesRegion, ResetKind},
    input::{JoypadBtn, Player},
    mapper::MapperRevision,
    video::VideoFilter,
};
use serde::{Deserialize, Serialize};

/// A user action that maps to a possible state change on [`ControlDeck`]. Used for event
/// handling and test abstractions.
///
/// [`ControlDeck`]: crate::control_deck::ControlDeck
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Reset(ResetKind),
    Joypad((Player, JoypadBtn)),
    ToggleZapperConnected,
    ZapperAim((u32, u32)),
    ZapperAimOffscreen,
    ZapperTrigger,
    SetSaveSlot(u8),
    SaveState,
    LoadState,
    ToggleApuChannel(Channel),
    MapperRevision(MapperRevision),
    SetNesRegion(NesRegion),
    SetVideoFilter(VideoFilter),
}
