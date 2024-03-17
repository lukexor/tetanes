use crate::{
    apu::Channel, common::NesRegion, input::JoypadBtn, mapper::MapperRevision, video::VideoFilter,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    SoftReset,
    HardReset,
    Joypad(JoypadBtn),
    ZapperTrigger,
    SetSaveSlot(u8),
    SaveState,
    LoadState,
    ToggleApuChannel(Channel),
    MapperRevision(MapperRevision),
    SetNesRegion(NesRegion),
    SetVideoFilter(VideoFilter),
}
