//! RetroPad to NES controller.
//!
//! A frontend reports the state of every button every frame, but [`Joypad::set_button`] is
//! edge-driven: releasing a turbo button deliberately clears the plain one it fires, so replaying
//! "turbo is not held" each frame would wipe A and B the instant they were pressed. What is sent
//! down is therefore the *difference* from last frame, which is also how the desktop UI drives it.

use crate::{log, sys};
use std::ffi::{CStr, c_void};
use tetanes_core::input::{Joypad, JoypadBtnState, Player};

/// How a RetroPad button maps onto the NES pad, and what the frontend's remapping menu calls it.
///
/// The two layouts are mirrored: what a RetroPad calls `B` sits where the NES `A` is, so a player
/// holding a modern controller presses the button in the place they expect. `X` and `Y` are dead on
/// an NES pad and carry turbo instead, which is what other NES cores do with them.
///
/// The description is the *NES* button, not the RetroPad one the frontend already knows - which is
/// what makes the mirroring visible to someone reading the menu rather than a surprise under it.
pub const BUTTONS: [(u32, JoypadBtnState, &CStr); 10] = [
    (sys::RETRO_DEVICE_ID_JOYPAD_B, JoypadBtnState::A, c"A"),
    (sys::RETRO_DEVICE_ID_JOYPAD_A, JoypadBtnState::B, c"B"),
    (
        sys::RETRO_DEVICE_ID_JOYPAD_Y,
        JoypadBtnState::TURBO_A,
        c"Turbo A",
    ),
    (
        sys::RETRO_DEVICE_ID_JOYPAD_X,
        JoypadBtnState::TURBO_B,
        c"Turbo B",
    ),
    (
        sys::RETRO_DEVICE_ID_JOYPAD_SELECT,
        JoypadBtnState::SELECT,
        c"Select",
    ),
    (
        sys::RETRO_DEVICE_ID_JOYPAD_START,
        JoypadBtnState::START,
        c"Start",
    ),
    (sys::RETRO_DEVICE_ID_JOYPAD_UP, JoypadBtnState::UP, c"Up"),
    (
        sys::RETRO_DEVICE_ID_JOYPAD_DOWN,
        JoypadBtnState::DOWN,
        c"Down",
    ),
    (
        sys::RETRO_DEVICE_ID_JOYPAD_LEFT,
        JoypadBtnState::LEFT,
        c"Left",
    ),
    (
        sys::RETRO_DEVICE_ID_JOYPAD_RIGHT,
        JoypadBtnState::RIGHT,
        c"Right",
    ),
];

/// Ports this core reads. Four-player lands with `retro_set_controller_port_device`.
pub const PORTS: [Player; 2] = [Player::One, Player::Two];

/// Tells the frontend what each button does, so its remapping menu names them.
///
/// Without this a player sees only the RetroPad's own labels, which is where the mirroring becomes
/// a surprise: the button marked `B` presses NES `A`, and nothing on screen says so.
///
/// # Safety
///
/// The environment callback must be valid.
pub unsafe fn describe(environment: sys::retro_environment_t) {
    let mut descriptors: Vec<sys::retro_input_descriptor> = PORTS
        .iter()
        .enumerate()
        .flat_map(|(port, _)| {
            BUTTONS.map(|(id, _, description)| sys::retro_input_descriptor {
                port: port as u32,
                device: sys::RETRO_DEVICE_JOYPAD,
                index: 0,
                id,
                description: description.as_ptr(),
            })
        })
        .collect();
    descriptors.push(sys::retro_input_descriptor {
        port: 0,
        device: 0,
        index: 0,
        id: 0,
        description: std::ptr::null(),
    });
    // SAFETY: the frontend reads until the null description, and every string is `'static`.
    let ok = unsafe {
        environment(
            sys::RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS,
            descriptors.as_mut_ptr().cast::<c_void>(),
        )
    };
    if !ok {
        // Not fatal: it costs the menu its labels, not the player their controls.
        log::info("the frontend does not take input descriptors");
    }
}

/// What each port reported last frame, so that only changes are sent to the console.
#[derive(Default)]
pub struct Pads {
    previous: [JoypadBtnState; PORTS.len()],
}

impl Pads {
    /// Sends the buttons that changed since last frame.
    pub fn apply(&mut self, port: usize, joypad: &mut Joypad, held: JoypadBtnState) {
        let changed = held.symmetric_difference(self.previous[port]);
        for (_, button, _) in BUTTONS {
            if changed.contains(button) {
                // Through `set_button` rather than assigning the bits, so SOCD filtering still
                // applies to the D-pad.
                joypad.set_button(button, held.contains(button));
            }
        }
        self.previous[port] = held;
    }

    /// Forgets what was held, so the next frame resends it.
    ///
    /// Needed wherever the console's own pads are cleared - a reset, or a cart change - since a
    /// button still physically held would otherwise look unchanged and never be sent again.
    pub const fn forget(&mut self) {
        self.previous = [JoypadBtnState::empty(); PORTS.len()];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A duplicate on either side would silently drop a button.
    #[test]
    fn the_mapping_is_one_to_one() {
        let mut ids: Vec<u32> = BUTTONS.iter().map(|&(id, ..)| id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), BUTTONS.len(), "a RetroPad id is mapped twice");

        let mut nes = BUTTONS
            .iter()
            .fold(JoypadBtnState::empty(), |acc, &(_, b, _)| {
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
        let map = |id| BUTTONS.iter().find(|&&(i, ..)| i == id).map(|&(_, b, _)| b);
        assert_eq!(map(sys::RETRO_DEVICE_ID_JOYPAD_B), Some(JoypadBtnState::A));
        assert_eq!(map(sys::RETRO_DEVICE_ID_JOYPAD_A), Some(JoypadBtnState::B));
    }

    /// Replaying every button's state each frame clears A and B, because `set_button` treats a
    /// turbo release as a release of the button it fires. Sending only changes is what avoids it,
    /// and holding a button across frames is the case that catches a regression.
    #[test]
    fn a_held_button_stays_held_across_frames() {
        let mut pads = Pads::default();
        let mut joypad = Joypad::new();

        for frame in 0..5 {
            pads.apply(0, &mut joypad, JoypadBtnState::A);
            assert!(
                joypad.button(JoypadBtnState::A),
                "A is still held on frame {frame}"
            );
        }

        pads.apply(0, &mut joypad, JoypadBtnState::empty());
        assert!(!joypad.button(JoypadBtnState::A), "and releasing it lands");
    }

    /// Turbo has to keep working, which is the behaviour the edge rule exists for.
    #[test]
    fn turbo_is_still_delivered() {
        let mut pads = Pads::default();
        let mut joypad = Joypad::new();

        pads.apply(0, &mut joypad, JoypadBtnState::TURBO_A);
        assert!(joypad.button(JoypadBtnState::TURBO_A));

        pads.apply(0, &mut joypad, JoypadBtnState::empty());
        assert!(!joypad.button(JoypadBtnState::TURBO_A));
        assert!(
            !joypad.button(JoypadBtnState::A),
            "releasing turbo clears the button it was firing"
        );
    }

    /// After the console's pads are cleared, a button still held has to be sent again.
    #[test]
    fn forgetting_resends_a_held_button() {
        let mut pads = Pads::default();
        let mut joypad = Joypad::new();

        pads.apply(0, &mut joypad, JoypadBtnState::START);
        joypad.clear(); // as a reset does
        assert!(!joypad.button(JoypadBtnState::START));

        pads.apply(0, &mut joypad, JoypadBtnState::START);
        assert!(!joypad.button(JoypadBtnState::START), "nothing changed");

        pads.forget();
        pads.apply(0, &mut joypad, JoypadBtnState::START);
        assert!(joypad.button(JoypadBtnState::START), "sent again");
    }
}
