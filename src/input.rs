//! NES Controller Inputs

use crate::memory::Memory;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// Alias for Input wrapped in a Rc/RefCell
pub type InputRef = Rc<RefCell<Input>>;

// The "strobe state": the order in which the NES reads the buttons.
const STROBE_A: u8 = 0;
const STROBE_B: u8 = 1;
const STROBE_SELECT: u8 = 2;
const STROBE_START: u8 = 3;
const STROBE_UP: u8 = 4;
const STROBE_DOWN: u8 = 5;
const STROBE_LEFT: u8 = 6;
const STROBE_RIGHT: u8 = 7;

/// Represents an NES Joypad
#[derive(Default, Debug)]
pub struct Gamepad {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub a: bool,
    pub b: bool,
    pub turbo_a: bool,
    pub turbo_b: bool,
    pub select: bool,
    pub start: bool,
    pub strobe_state: u8,
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
    fn reset(&mut self) {
        self.strobe_state = STROBE_A;
    }
}

/// Input containing gamepad input state
#[derive(Default)]
pub struct Input {
    pub gamepad1: Gamepad,
    pub gamepad2: Gamepad,
}

impl Input {
    /// Returns an empty Input instance with no event pump
    pub fn new() -> Self {
        Self {
            gamepad1: Gamepad::default(),
            gamepad2: Gamepad::default(),
        }
    }
}

impl Memory for Input {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x4016 => self.gamepad1.next_state(),
            0x4017 => self.gamepad2.next_state(),
            _ => 0,
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x4016 => self.gamepad1.peek_state(),
            0x4017 => self.gamepad2.peek_state(),
            _ => 0,
        }
    }

    fn write(&mut self, addr: u16, _val: u8) {
        if addr == 0x4016 {
            self.gamepad1.reset();
            self.gamepad2.reset();
        }
    }
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Input {{ }} ")
    }
}
