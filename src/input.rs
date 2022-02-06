//! NES Controller Inputs

use crate::{
    common::Powered,
    memory::{MemRead, MemWrite},
};
use serde::{Deserialize, Serialize};
use std::fmt;

// The "strobe state": the order in which the NES reads the buttons.
const STROBE_A: u8 = 0;
const STROBE_B: u8 = 1;
const STROBE_SELECT: u8 = 2;
const STROBE_START: u8 = 3;
const STROBE_UP: u8 = 4;
const STROBE_DOWN: u8 = 5;
const STROBE_LEFT: u8 = 6;
const STROBE_RIGHT: u8 = 7;

/// A NES Gamepad slot.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadSlot {
    /// Player one
    One,
    /// Player two
    Two,
}

/// A NES Gamepad.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GamepadBtn {
    /// Left D-Pad.
    Left,
    /// Right D-Pad.
    Right,
    /// Up D-Pad.
    Up,
    /// Down D-Pad.
    Down,
    /// A Button.
    A,
    /// B Button.
    B,
    /// A Button (Turbo).
    TurboA,
    /// B Button (Turbo).
    TurboB,
    /// Select Button.
    Select,
    /// Start Button.
    Start,
    /// Zapper Trigger.
    Zapper,
}

impl AsRef<str> for GamepadBtn {
    fn as_ref(&self) -> &str {
        match self {
            GamepadBtn::Left => "Left",
            GamepadBtn::Right => "Right",
            GamepadBtn::Up => "Up",
            GamepadBtn::Down => "Down",
            GamepadBtn::A => "A",
            GamepadBtn::TurboA => "A (Turbo)",
            GamepadBtn::B => "B",
            GamepadBtn::TurboB => "B (Turbo)",
            GamepadBtn::Select => "Select",
            GamepadBtn::Start => "Start",
            GamepadBtn::Zapper => "Zapper Trigger",
        }
    }
}

/// Represents an NES Joypad
#[derive(Default, Debug, Copy, Clone)]
pub struct Gamepad {
    /// Left D-Pad pressed or not.
    pub left: bool,
    /// Right D-Pad pressed or not.
    pub right: bool,
    /// Up D-Pad pressed or not.
    pub up: bool,
    /// Down D-Pad pressed or not.
    pub down: bool,
    /// A Button pressed or not.
    pub a: bool,
    /// B Button pressed or not.
    pub b: bool,
    /// A Button (Turbo) pressed or not.
    pub turbo_a: bool,
    /// B Button (Turbo) pressed or not.
    pub turbo_b: bool,
    /// Select Button pressed or not.
    pub select: bool,
    /// Start Button pressed or not.
    pub start: bool,
    /// Current strobe state. This is the shift register position for which gamepad button to read
    /// this tick.
    pub strobe_state: u8,
}

#[derive(Default, Debug, Copy, Clone)]
pub struct Zapper {
    pub light_sense: bool,
    pub triggered: bool,
}

impl Gamepad {
    fn next_state(&mut self) -> u8 {
        let state = match self.strobe_state {
            STROBE_A => self.a,
            STROBE_B => self.b,
            STROBE_SELECT => self.select,
            STROBE_START => self.start,
            STROBE_UP => self.up,
            STROBE_DOWN => self.down,
            STROBE_LEFT => self.left,
            STROBE_RIGHT => self.right,
            _ => panic!("invalid state {}", self.strobe_state),
        };
        self.strobe_state = (self.strobe_state + 1) & 7;
        state as u8
    }

    fn peek_state(&self) -> u8 {
        let state = match self.strobe_state {
            STROBE_A => self.a,
            STROBE_B => self.b,
            STROBE_SELECT => self.select,
            STROBE_START => self.start,
            STROBE_UP => self.up,
            STROBE_DOWN => self.down,
            STROBE_LEFT => self.left,
            STROBE_RIGHT => self.right,
            _ => panic!("invalid state {}", self.strobe_state),
        };
        state as u8
    }
}

impl Powered for Gamepad {
    fn reset(&mut self) {
        self.strobe_state = STROBE_A;
    }
}

/// Input containing gamepad input state
#[derive(Default, Copy, Clone)]
pub struct Input {
    pub gamepad1: Gamepad,
    pub gamepad2: Gamepad,
    pub zapper: Zapper,
    open_bus: u8,
}

impl Input {
    /// Returns an empty Input instance with no event pump
    pub fn new() -> Self {
        Self {
            gamepad1: Gamepad::default(),
            gamepad2: Gamepad::default(),
            zapper: Zapper::default(),
            open_bus: 0u8,
        }
    }
}

impl MemRead for Input {
    fn read(&mut self, addr: u16) -> u8 {
        let val = match addr {
            0x4016 => self.gamepad1.next_state() | 0x40,
            0x4017 => self.gamepad2.next_state() | 0x40,
            _ => self.open_bus,
        };
        self.open_bus = val;
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x4016 => self.gamepad1.peek_state() | 0x40,
            0x4017 => self.gamepad2.peek_state() | 0x40,
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Input {
    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        if addr == 0x4016 && val == 0 {
            self.gamepad1.reset();
            self.gamepad2.reset();
        }
    }
}

impl Powered for Input {
    fn reset(&mut self) {
        self.gamepad1.reset();
        self.gamepad2.reset();
    }
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        write!(f, "Input {{ }} ")
    }
}
