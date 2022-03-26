//! NES Controller Inputs

use crate::{
    common::Powered,
    memory::{MemRead, MemWrite},
    ppu::{Ppu, RENDER_HEIGHT, RENDER_WIDTH},
};
use pix_engine::shape::Point;
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
const STROBE_MAX: u8 = 8;

/// A NES Gamepad slot.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum GamepadSlot {
    /// Player one
    One,
    /// Player two
    Two,
    /// Player three
    Three,
    /// Player four
    Four,
}

/// A NES Gamepad.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[must_use]
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
#[must_use]
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
    pub strobe: u8,
}

impl Gamepad {
    #[inline]
    fn read(&mut self) -> u8 {
        let state = self.peek();
        if self.strobe <= 7 {
            self.strobe += 1;
        }
        state
    }

    #[inline]
    fn peek(&self) -> u8 {
        let state = match self.strobe {
            STROBE_A => self.a,
            STROBE_B => self.b,
            STROBE_SELECT => self.select,
            STROBE_START => self.start,
            STROBE_UP => self.up,
            STROBE_DOWN => self.down,
            STROBE_LEFT => self.left,
            STROBE_RIGHT => self.right,
            _ => true,
        };
        state as u8
    }
}

impl Powered for Gamepad {
    fn reset(&mut self) {
        self.strobe = STROBE_A;
    }
}

#[derive(Default, Debug, Copy, Clone)]
#[must_use]
pub struct Signature {
    signature: u8,
    strobe: u8,
}

impl Signature {
    fn new(signature: u8) -> Self {
        Self {
            signature,
            strobe: 0x00,
        }
    }

    #[inline]
    fn read(&mut self) -> u8 {
        let state = self.peek();
        if self.strobe <= 7 {
            self.strobe += 1;
        }
        state
    }

    #[inline]
    fn peek(&self) -> u8 {
        if self.strobe == STROBE_MAX {
            0x01
        } else {
            (self.signature >> self.strobe) & 0x01
        }
    }
}

impl Powered for Signature {
    fn reset(&mut self) {
        self.strobe = 0x00;
    }
}

#[derive(Default, Debug, Copy, Clone)]
#[must_use]
pub struct Zapper {
    pub triggered: u8,
    pub pos: Point<i32>,
    pub radius: i32,
    pub connected: bool,
}

impl Zapper {
    pub fn trigger(&mut self) {
        if self.triggered == 0 {
            self.triggered = 7;
        }
    }

    pub fn update(&mut self) {
        if self.triggered > 0 {
            self.triggered -= 1;
        }
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }
}

impl Zapper {
    fn new() -> Self {
        Self {
            triggered: 0,
            pos: Point::default(),
            radius: 3,
            connected: false,
        }
    }

    fn read(&self, ppu: &Ppu) -> u8 {
        self.triggered() | self.light_sense(ppu) | 0x40
    }

    fn triggered(&self) -> u8 {
        if self.triggered > 0 {
            0x10
        } else {
            0x00
        }
    }

    fn light_sense(&self, ppu: &Ppu) -> u8 {
        let width = RENDER_WIDTH as i32;
        let height = RENDER_HEIGHT as i32;
        // EXPL: Light sense is 1 scanline delayed for, likely due to slightly inaccurate NMI timing
        let scanline = ppu.scanline as i32 - 1;
        let cycle = ppu.cycle as i32;
        let [x, y] = self.pos.as_array();
        if x >= 0 && y >= 0 {
            for y in (y - self.radius)..=(y + self.radius) {
                if y >= 0 && y < height {
                    for x in (x - self.radius)..=(x + self.radius) {
                        let in_bounds = x >= 0 && x < width;
                        let behind_ppu =
                            scanline >= y && (scanline - y) <= 20 && (scanline != y || cycle > x);
                        if in_bounds
                            && behind_ppu
                            && ppu.get_pixel_brightness(x as u32, y as u32) >= 85
                        {
                            return 0x00;
                        }
                    }
                }
            }
        }
        0x08
    }
}

/// Input containing gamepad input state
#[derive(Default, Copy, Clone)]
#[must_use]
pub struct Input {
    pub gamepads: [Gamepad; 4],
    pub signatures: [Signature; 2],
    pub zapper: Zapper,
    pub shift_strobe: u8,
    pub open_bus: u8,
}

impl Input {
    /// Returns an empty Input instance with no event pump
    pub fn new() -> Self {
        Self {
            gamepads: [Gamepad::default(); 4],
            // Signature bits are reversed so they can shift right
            signatures: [Signature::new(0b00001000), Signature::new(0b00000100)],
            zapper: Zapper::new(),
            shift_strobe: 0x00,
            open_bus: 0x00,
        }
    }

    pub fn read_zapper(&self, ppu: &Ppu) -> u8 {
        self.zapper.read(ppu)
    }
}

impl MemRead for Input {
    #[inline]
    fn read(&mut self, addr: u16) -> u8 {
        let val = match addr {
            0x4016 => {
                if self.shift_strobe == 0x01 {
                    self.reset();
                }
                // Read $4016 D0 8x for controller #1.
                // Read $4016 D0 8x for controller #3.
                // Read $4016 D0 8x for signature: 0b00010000
                if self.gamepads[0].strobe < STROBE_MAX {
                    self.gamepads[0].read()
                } else if self.gamepads[2].strobe < STROBE_MAX {
                    self.gamepads[2].read()
                } else if self.signatures[0].strobe < STROBE_MAX {
                    self.signatures[0].read()
                } else {
                    0x01
                }
            }
            0x4017 => {
                if self.shift_strobe == 0x01 {
                    self.reset();
                }
                // Read $4017 D0 8x for controller #2.
                // Read $4017 D0 8x for controller #4.
                // Read $4017 D0 8x for signature: 0b00100000
                if self.gamepads[1].strobe < STROBE_MAX {
                    self.gamepads[1].read()
                } else if self.gamepads[3].strobe < STROBE_MAX {
                    self.gamepads[3].read()
                } else if self.signatures[1].strobe < STROBE_MAX {
                    self.signatures[1].read()
                } else {
                    0x01
                }
            }
            _ => self.open_bus,
        };
        self.open_bus = val;
        val | 0x40
    }

    #[inline]
    fn peek(&self, addr: u16) -> u8 {
        let val = match addr {
            0x4016 => {
                if self.gamepads[0].strobe < STROBE_MAX {
                    self.gamepads[0].peek()
                } else if self.gamepads[2].strobe < STROBE_MAX {
                    self.gamepads[2].peek()
                } else if self.signatures[0].strobe < STROBE_MAX {
                    self.signatures[0].peek()
                } else {
                    0x01
                }
            }
            0x4017 => {
                if self.gamepads[1].strobe < STROBE_MAX {
                    self.gamepads[1].peek()
                } else if self.gamepads[3].strobe < STROBE_MAX {
                    self.gamepads[3].peek()
                } else if self.signatures[1].strobe < STROBE_MAX {
                    self.signatures[1].peek()
                } else {
                    0x01
                }
            }
            _ => self.open_bus,
        };
        val | 0x40
    }
}

impl MemWrite for Input {
    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        if addr == 0x4016 {
            let prev_strobe = self.shift_strobe;
            self.shift_strobe = val & 0x01;
            if prev_strobe == 0x01 && self.shift_strobe == 0x00 {
                self.reset();
            }
        }
    }
}

impl Powered for Input {
    fn reset(&mut self) {
        for gamepad in &mut self.gamepads {
            gamepad.reset();
        }
        for signature in &mut self.signatures {
            signature.reset();
        }
    }
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        write!(f, "Input {{ }} ")
    }
}
