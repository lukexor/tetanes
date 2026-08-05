//! RetroPad to NES controller.

use crate::sys;
use tetanes_core::input::JoypadBtnState;

/// How a RetroPad button maps onto the NES pad.
///
/// The two layouts are mirrored: what a RetroPad calls `B` sits where the NES `A` is, so a player
/// holding a modern controller presses the button in the place they expect. `X` and `Y` are dead on
/// an NES pad and carry turbo instead, which is what other NES cores do with them.
pub const BUTTONS: [(u32, JoypadBtnState); 10] = [
    (sys::RETRO_DEVICE_ID_JOYPAD_B, JoypadBtnState::A),
    (sys::RETRO_DEVICE_ID_JOYPAD_A, JoypadBtnState::B),
    (sys::RETRO_DEVICE_ID_JOYPAD_Y, JoypadBtnState::TURBO_A),
    (sys::RETRO_DEVICE_ID_JOYPAD_X, JoypadBtnState::TURBO_B),
    (sys::RETRO_DEVICE_ID_JOYPAD_SELECT, JoypadBtnState::SELECT),
    (sys::RETRO_DEVICE_ID_JOYPAD_START, JoypadBtnState::START),
    (sys::RETRO_DEVICE_ID_JOYPAD_UP, JoypadBtnState::UP),
    (sys::RETRO_DEVICE_ID_JOYPAD_DOWN, JoypadBtnState::DOWN),
    (sys::RETRO_DEVICE_ID_JOYPAD_LEFT, JoypadBtnState::LEFT),
    (sys::RETRO_DEVICE_ID_JOYPAD_RIGHT, JoypadBtnState::RIGHT),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A duplicate on either side would silently drop a button.
    #[test]
    fn the_mapping_is_one_to_one() {
        let mut ids: Vec<u32> = BUTTONS.iter().map(|&(id, _)| id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), BUTTONS.len(), "a RetroPad id is mapped twice");

        let mut nes = BUTTONS
            .iter()
            .fold(JoypadBtnState::empty(), |acc, &(_, b)| {
                assert!(!acc.contains(b), "{b:?} is mapped twice");
                acc | b
            });
        nes.remove(JoypadBtnState::TURBO_A | JoypadBtnState::TURBO_B);
        assert_eq!(
            nes,
            JoypadBtnState::all() - JoypadBtnState::TURBO_A - JoypadBtnState::TURBO_B,
            "every real NES button is reachable"
        );
    }

    /// The mirroring is the part a reader will assume is a typo, so it is asserted.
    #[test]
    fn the_face_buttons_are_mirrored() {
        let map = |id| BUTTONS.iter().find(|&&(i, _)| i == id).map(|&(_, b)| b);
        assert_eq!(map(sys::RETRO_DEVICE_ID_JOYPAD_B), Some(JoypadBtnState::A));
        assert_eq!(map(sys::RETRO_DEVICE_ID_JOYPAD_A), Some(JoypadBtnState::B));
    }
}
