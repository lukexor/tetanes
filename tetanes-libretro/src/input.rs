//! RetroPad to NES controller.
//!
//! A frontend reports the state of every button every frame, but [`Joypad::set_button`] is
//! edge-driven: releasing a turbo button deliberately clears the plain one it fires, so replaying
//! "turbo is not held" each frame would wipe A and B the instant they were pressed. What is sent
//! down is therefore the *difference* from last frame, which is also how the desktop UI drives it.

use crate::{log, sys};
use std::ffi::{CStr, c_uint, c_void};
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

/// Ports this core reads.
///
/// Four, always. Three and four exist only behind an adapter, and the console ignores them unless
/// `tetanes_four_player` has plugged one in - but the *ports* are declared regardless, because a
/// frontend has to be able to assign controllers to them before the option is set.
pub const PORTS: [Player; 4] = [Player::One, Player::Two, Player::Three, Player::Four];

/// The port the Zapper plugs into, as it does on the hardware: `$4017`, which port two shares.
pub const ZAPPER_PORT: usize = 1;

/// Aimed here, the light sense reads dark whatever is on screen.
///
/// [`Zapper::light_sense`](tetanes_core::input::Zapper) samples a radius around the aim point and
/// clamps the far edge to the frame, so a Y far enough below it leaves an empty range and nothing
/// is sampled at all. That is what a shot fired off-screen has to look like: the trigger pulled,
/// no light seen.
pub const OFFSCREEN_Y: u16 = 250;

/// What the frontend has plugged into one port.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// Nothing, so the port is not polled at all.
    None,
    #[default]
    Joypad,
    /// The Zapper, which only [`ZAPPER_PORT`] accepts.
    Zapper,
}

impl Device {
    /// Reads what a frontend asked for, ignoring a device this port has no socket for.
    fn from_retro(port: usize, device: c_uint) -> Self {
        match device & sys::RETRO_DEVICE_MASK {
            sys::RETRO_DEVICE_NONE => Self::None,
            sys::RETRO_DEVICE_LIGHTGUN if port == ZAPPER_PORT => Self::Zapper,
            sys::RETRO_DEVICE_LIGHTGUN => {
                log::info("the Zapper plugs into port 2, as it does on the console");
                Self::None
            }
            _ => Self::Joypad,
        }
    }
}

/// Lets a table of C structs holding pointers live in a `static`.
///
/// What `Sync` objects to is the raw pointer, not what it points at: every one below is a string
/// literal or another `static` here, so all of them are valid for the life of the process.
struct Table<T>(T);

// SAFETY: immutable, and every pointer inside is to `'static` data.
unsafe impl<T> Sync for Table<T> {}

/// Devices each port accepts, terminated by a zeroed entry.
///
/// **`static`, not a local.** `SET_CONTROLLER_INFO` is the one call in this crate whose data the
/// frontend does not copy: RetroArch memcpy's the `retro_controller_info` array and keeps the
/// `types` pointer inside it as-is, so a list built on the stack or the heap dangles the moment
/// this returns and its controls menu reads freed memory. C cores write these as `static const`
/// for exactly this reason.
static JOYPAD_ONLY: Table<[sys::retro_controller_description; 2]> = Table([
    sys::retro_controller_description {
        desc: c"Controller".as_ptr(),
        id: sys::RETRO_DEVICE_JOYPAD,
    },
    sys::retro_controller_description {
        desc: std::ptr::null(),
        id: 0,
    },
]);

static JOYPAD_OR_ZAPPER: Table<[sys::retro_controller_description; 3]> = Table([
    sys::retro_controller_description {
        desc: c"Controller".as_ptr(),
        id: sys::RETRO_DEVICE_JOYPAD,
    },
    sys::retro_controller_description {
        desc: c"Zapper".as_ptr(),
        id: sys::RETRO_DEVICE_LIGHTGUN,
    },
    sys::retro_controller_description {
        desc: std::ptr::null(),
        id: 0,
    },
]);

/// One entry per port, in port order, terminated by a zeroed entry.
static PORT_DEVICES: Table<[sys::retro_controller_info; PORTS.len() + 1]> = Table([
    sys::retro_controller_info {
        types: JOYPAD_ONLY.0.as_ptr(),
        num_types: 1,
    },
    // Port two, the one with the Zapper's socket.
    sys::retro_controller_info {
        types: JOYPAD_OR_ZAPPER.0.as_ptr(),
        num_types: 2,
    },
    sys::retro_controller_info {
        types: JOYPAD_ONLY.0.as_ptr(),
        num_types: 1,
    },
    sys::retro_controller_info {
        types: JOYPAD_ONLY.0.as_ptr(),
        num_types: 1,
    },
    sys::retro_controller_info {
        types: std::ptr::null(),
        num_types: 0,
    },
]);

/// Tells the frontend what each port accepts, so its controller menu offers the Zapper on the port
/// that has one and nothing on the ports that do not.
///
/// # Safety
///
/// The environment callback must be valid.
pub unsafe fn declare_ports(environment: sys::retro_environment_t) {
    // SAFETY: the frontend reads until the null `types` and keeps the pointers it finds, which is
    // why what they point at is `static`.
    let ok = unsafe {
        environment(
            sys::RETRO_ENVIRONMENT_SET_CONTROLLER_INFO,
            PORT_DEVICES.0.as_ptr().cast_mut().cast::<c_void>(),
        )
    };
    if !ok {
        log::info("the frontend does not take controller descriptions");
    }
}

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
    // The Zapper's own buttons, on the one port that takes it.
    for (id, description) in [
        (sys::RETRO_DEVICE_ID_LIGHTGUN_TRIGGER, c"Trigger"),
        (sys::RETRO_DEVICE_ID_LIGHTGUN_RELOAD, c"Shoot Off-screen"),
    ] {
        descriptors.push(sys::retro_input_descriptor {
            port: ZAPPER_PORT as u32,
            device: sys::RETRO_DEVICE_LIGHTGUN,
            index: 0,
            id,
            description: description.as_ptr(),
        });
    }
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

/// What is in each port, and what it reported last frame.
pub struct Pads {
    devices: [Device; PORTS.len()],
    previous: [JoypadBtnState; PORTS.len()],
    /// Whether the Zapper's trigger was held last frame.
    ///
    /// The pull is an edge: [`Zapper::trigger`](tetanes_core::input::Zapper) arms a timer that
    /// releases itself after ~100 ms, so re-arming it every frame a held trigger is reported would
    /// turn one shot into ten a second.
    trigger_held: bool,
}

impl Default for Pads {
    fn default() -> Self {
        Self {
            // Two controllers, which is what a console has without an adapter and what a frontend
            // that never calls `retro_set_controller_port_device` should still get.
            devices: [Device::Joypad, Device::Joypad, Device::None, Device::None],
            previous: [JoypadBtnState::empty(); PORTS.len()],
            trigger_held: false,
        }
    }
}

impl Pads {
    /// Records what the frontend plugged into a port. Out-of-range ports are ignored.
    pub fn set_device(&mut self, port: usize, device: c_uint) {
        let Some(slot) = self.devices.get_mut(port) else {
            return;
        };
        let device = Device::from_retro(port, device);
        if *slot != device {
            *slot = device;
            // Whatever was held on the old device is not held on the new one.
            self.previous[port] = JoypadBtnState::empty();
            self.trigger_held = false;
        }
    }

    /// What is in a port.
    pub fn device(&self, port: usize) -> Device {
        self.devices.get(port).copied().unwrap_or_default()
    }

    /// Whether any port has the Zapper in it.
    pub fn zapper_connected(&self) -> bool {
        self.devices.contains(&Device::Zapper)
    }

    /// Whether this frame's trigger report is a fresh pull rather than one still being held.
    pub const fn pulled(&mut self, held: bool) -> bool {
        let pulled = held && !self.trigger_held;
        self.trigger_held = held;
        pulled
    }
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
        self.trigger_held = false;
    }
}

/// Turns a light gun's absolute axis into a frame coordinate.
///
/// libretro spans `-0x8000..=0x7FFF` across the viewport whatever its size, so the frame's own
/// dimensions are the only scale needed.
pub fn aim_to_pixel(axis: i16, span: u16) -> u16 {
    let offset = i32::from(axis) + 0x8000;
    let pixel = (offset * i32::from(span)) / 0x1_0000;
    pixel.clamp(0, i32::from(span) - 1) as u16
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

    /// What `SET_CONTROLLER_INFO` hands over has to outlive the call: RetroArch copies the outer
    /// array and keeps the `types` pointer inside it, so a list built on the heap dangles the
    /// moment `declare_ports` returns, and its controls menu reads freed memory - which is a crash,
    /// and was.
    ///
    /// A `static` has the same address every time; anything allocated per call does not.
    #[test]
    fn the_port_devices_outlive_the_call_that_hands_them_over() {
        let first: Vec<_> = PORT_DEVICES.0.iter().map(|info| info.types).collect();
        let second: Vec<_> = PORT_DEVICES.0.iter().map(|info| info.types).collect();
        assert_eq!(first, second, "not the same memory twice");

        assert_eq!(
            PORT_DEVICES.0.len(),
            PORTS.len() + 1,
            "one per port, plus the terminator"
        );
        let last = PORT_DEVICES.0.last().expect("a terminator");
        assert!(last.types.is_null(), "the array has to end somewhere");

        for (port, info) in PORT_DEVICES.0.iter().enumerate().take(PORTS.len()) {
            assert!(!info.types.is_null(), "port {port}");
            // SAFETY: each list is a `static` of `num_types` entries plus a null terminator.
            let types = unsafe { std::slice::from_raw_parts(info.types, info.num_types as usize) };
            assert!(
                types.iter().all(|ty| !ty.desc.is_null()),
                "port {port} counts its own terminator"
            );
            // SAFETY: as above; the entry past the counted ones is the terminator.
            let terminator = unsafe { (*info.types.add(info.num_types as usize)).desc };
            assert!(terminator.is_null(), "port {port} is not terminated");

            let offers_zapper = types.iter().any(|ty| ty.id == sys::RETRO_DEVICE_LIGHTGUN);
            assert_eq!(
                offers_zapper,
                port == ZAPPER_PORT,
                "port {port} offers the Zapper"
            );
        }
    }

    /// The Zapper has one socket on the console, and a frontend that asks for it elsewhere has to
    /// be refused rather than quietly given a second gun.
    #[test]
    fn the_zapper_only_fits_the_port_that_has_a_socket() {
        assert_eq!(
            Device::from_retro(ZAPPER_PORT, sys::RETRO_DEVICE_LIGHTGUN),
            Device::Zapper
        );
        for port in [0, 2, 3] {
            assert_eq!(
                Device::from_retro(port, sys::RETRO_DEVICE_LIGHTGUN),
                Device::None,
                "port {port}"
            );
        }
        assert_eq!(Device::from_retro(0, sys::RETRO_DEVICE_NONE), Device::None);
        // A frontend may hand over a subclass, whose low byte is still the base device.
        assert_eq!(
            Device::from_retro(0, sys::RETRO_DEVICE_JOYPAD | (1 << 8)),
            Device::Joypad
        );
    }

    /// A console has two ports without an adapter, so that is what a frontend which never assigns
    /// anything gets.
    #[test]
    fn two_controllers_are_plugged_in_to_begin_with() {
        let pads = Pads::default();
        assert_eq!(pads.device(0), Device::Joypad);
        assert_eq!(pads.device(1), Device::Joypad);
        assert_eq!(pads.device(2), Device::None);
        assert_eq!(pads.device(3), Device::None);
        assert!(!pads.zapper_connected());
        // Out of range answers rather than panicking, since the frontend picks the number.
        assert_eq!(pads.device(9), Device::Joypad);
    }

    /// The whole span maps onto the frame, and the ends land inside it rather than one past.
    #[test]
    fn an_absolute_axis_becomes_a_frame_coordinate() {
        assert_eq!(aim_to_pixel(i16::MIN, 256), 0, "hard left");
        assert_eq!(aim_to_pixel(i16::MAX, 256), 255, "hard right, still inside");
        assert_eq!(aim_to_pixel(0, 256), 128, "the middle");
        assert_eq!(aim_to_pixel(i16::MIN, 240), 0);
        assert_eq!(aim_to_pixel(i16::MAX, 240), 239);
    }

    /// The trigger is a pull, not a hold: the console releases it after ~100 ms of its own accord,
    /// so re-arming every frame would turn one shot into ten a second.
    #[test]
    fn the_trigger_fires_once_per_pull() {
        let mut pads = Pads::default();
        assert!(pads.pulled(true), "the pull");
        assert!(!pads.pulled(true), "still held is not another shot");
        assert!(!pads.pulled(false), "nor is the release");
        assert!(pads.pulled(true), "the next pull is");

        pads.forget();
        assert!(pads.pulled(true), "and a reset re-arms it");
    }

    /// Changing what is in a port must not leave the last device's buttons looking held.
    #[test]
    fn swapping_a_device_forgets_what_it_was_holding() {
        let mut pads = Pads::default();
        let mut joypad = Joypad::new();
        pads.apply(0, &mut joypad, JoypadBtnState::START);

        pads.set_device(0, sys::RETRO_DEVICE_NONE);
        pads.set_device(0, sys::RETRO_DEVICE_JOYPAD);
        joypad.clear();

        pads.apply(0, &mut joypad, JoypadBtnState::START);
        assert!(
            joypad.button(JoypadBtnState::START),
            "sent again on the new device"
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
