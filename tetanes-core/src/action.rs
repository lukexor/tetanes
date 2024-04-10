use crate::{
    apu::Channel, common::NesRegion, input::JoypadBtn, mapper::MapperRevision, video::VideoFilter,
};
use serde::{Deserialize, Serialize};

/// A user action that maps to a possible state change on [`ControlDeck`]. Used for event
/// handling and test abstractions.
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    SoftReset,
    HardReset,
    Joypad(JoypadBtn),
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
