//! An [`Action`] is an enumerated list of possible state changes to the [`ControlDeck`] instance that
//! allows for event handling and test abstractions such as being able to map a custom keybind to a
//! given state change.
//!
//! [`ControlDeck`]: crate::control_deck::ControlDeck

use crate::{
    apu::Channel,
    common::{NesRegion, ResetKind},
    input::{FourPlayer, JoypadBtn, Player},
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
    /// Reset the [`ControlDeck`](crate::control_deck::ControlDeck).
    Reset(ResetKind),
    /// Update the [`Joypad`](crate::input::Joypad) button state.
    Joypad((Player, JoypadBtn)),
    /// Toggle the [`Zapper`](crate::input::Zapper) connected state.
    ToggleZapperConnected,
    /// Update the [`Zapper`](crate::input::Zapper) aim position.
    ZapperAim((u32, u32)),
    /// Update the [`Zapper`](crate::input::Zapper) aim position to offscreen.
    ZapperAimOffscreen,
    /// Trigger the [`Zapper`](crate::input::Zapper) trigger.
    ZapperTrigger,
    /// Set [`FourPlayer`] mode.
    FourPlayer(FourPlayer),
    /// Set the slot to use for save states.
    SetSaveSlot(u8),
    /// Save the current state to the currently set save slot.
    SaveState,
    /// Load the current state from the currently set save slot.
    LoadState,
    /// Toggle the [`Apu`](crate::apu::Apu) [`Channel`].
    ToggleApuChannel(Channel),
    /// Set the [`MapperRevision`].
    MapperRevision(MapperRevision),
    /// Set the [`NesRegion`].
    SetNesRegion(NesRegion),
    /// Set the [`VideoFilter`].
    SetVideoFilter(VideoFilter),
}
