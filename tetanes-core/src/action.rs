use crate::{
    apu::Channel, common::NesRegion, common::ResetKind, input::JoypadBtn, mapper::MapperRevision,
    video::VideoFilter,
};
use serde::{Deserialize, Serialize};

/// A user action that maps to a possible state change on [`ControlDeck`]. Used for event
/// handling and test abstractions.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Reset(ResetKind),
    Joypad(JoypadBtn),
    ToggleZapperConnected,
    ZapperConnect(bool),
    ZapperAim((u32, u32)),
    ZapperTrigger,
    SetSaveSlot(u8),
    SaveState,
    LoadState,
    ToggleApuChannel(Channel),
    MapperRevision(MapperRevision),
    SetNesRegion(NesRegion),
    SetVideoFilter(VideoFilter),
}
