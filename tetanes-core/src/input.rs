use crate::{
    common::{Clock, NesRegion, Reset, ResetKind},
    cpu::Cpu,
    ppu::Ppu,
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Player {
    #[default]
    One,
    Two,
    Three,
    Four,
}

impl AsRef<str> for Player {
    fn as_ref(&self) -> &str {
        match self {
            Self::One => "one",
            Self::Two => "two",
            Self::Three => "three",
            Self::Four => "four",
        }
    }
}

pub trait InputRegisters {
    fn read(&mut self, player: Player, ppu: &Ppu) -> u8;
    fn peek(&self, player: Player, ppu: &Ppu) -> u8;
    fn write(&mut self, val: u8);
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum FourPlayer {
    #[default]
    Disabled,
    FourScore,
    Satellite,
}

impl FourPlayer {
    pub const fn as_slice() -> &'static [Self] {
        &[Self::Disabled, Self::FourScore, Self::Satellite]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::FourScore => "four-score",
            Self::Satellite => "satellite",
        }
    }
}

impl AsRef<str> for FourPlayer {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for FourPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl FromStr for FourPlayer {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "disabled" => Ok(Self::Disabled),
            "four-score" => Ok(Self::FourScore),
            "satellite" => Ok(Self::Satellite),
            _ => Err(
                "invalid FourPlayer value. valid options: `disabled`, `four-score`, or `satellite`",
            ),
        }
    }
}

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Input {
    joypads: [Joypad; 4],
    signatures: [Joypad; 2],
    pub zapper: Zapper,
    turbo_timer: u32,
    pub four_player: FourPlayer,
}

impl Input {
    pub fn new() -> Self {
        Self {
            joypads: [Joypad::new(); 4],
            // Signature bits are reversed so they can shift right
            signatures: [
                Joypad::from_bytes(0b0000_1000),
                Joypad::from_bytes(0b0000_0100),
            ],
            zapper: Zapper::new(),
            turbo_timer: 30,
            four_player: FourPlayer::default(),
        }
    }

    pub const fn joypad(&self, player: Player) -> &Joypad {
        &self.joypads[player as usize]
    }

    pub fn joypad_mut(&mut self, player: Player) -> &mut Joypad {
        &mut self.joypads[player as usize]
    }

    pub fn connect_zapper(&mut self, connected: bool) {
        self.zapper.connected = connected;
    }

    pub fn set_four_player(&mut self, four_player: FourPlayer) {
        self.four_player = four_player;
        self.reset(ResetKind::Hard);
    }

    pub fn clear(&mut self) {
        for pad in &mut self.joypads {
            pad.clear();
        }
        self.zapper.clear();
    }
}

impl InputRegisters for Input {
    fn read(&mut self, player: Player, ppu: &Ppu) -> u8 {
        // Read $4016/$4017 D0 8x for controller #1/#2.
        // Read $4016/$4017 D0 8x for controller #3/#4.
        // Read $4016/$4017 D0 8x for signature: 0b00010000/0b00100000
        let zapper = if player == Player::Two {
            self.zapper.read(ppu)
        } else {
            0x00
        };

        let player = player as usize;
        assert!(player < 4);
        let val = match self.four_player {
            FourPlayer::Disabled => self.joypads[player].read(),
            FourPlayer::FourScore => {
                if self.joypads[player].index() < 8 {
                    self.joypads[player].read()
                } else if self.joypads[player + 2].index() < 8 {
                    self.joypads[player + 2].read()
                } else if self.signatures[player].index() < 8 {
                    self.signatures[player].read()
                } else {
                    0x01
                }
            }
            FourPlayer::Satellite => {
                self.joypads[player].read() | (self.joypads[player + 2].read() << 1)
            }
        };

        zapper | val | 0x40
    }

    fn peek(&self, player: Player, ppu: &Ppu) -> u8 {
        // Read $4016/$4017 D0 8x for controller #1/#2.
        // Read $4016/$4017 D0 8x for controller #3/#4.
        // Read $4016/$4017 D0 8x for signature: 0b00010000/0b00100000
        let zapper = if player == Player::Two {
            self.zapper.read(ppu)
        } else {
            0x00
        };

        let player = player as usize;
        assert!(player < 4);
        let val = match self.four_player {
            FourPlayer::Disabled => self.joypads[player].peek(),
            FourPlayer::FourScore => {
                if self.joypads[player].index() < 8 {
                    self.joypads[player].peek()
                } else if self.joypads[player + 2].index() < 8 {
                    self.joypads[player + 2].peek()
                } else if self.signatures[player].index() < 8 {
                    self.signatures[player].peek()
                } else {
                    0x01
                }
            }
            FourPlayer::Satellite => {
                self.joypads[player].peek() | (self.joypads[player + 2].peek() << 1)
            }
        };

        zapper | val | 0x40
    }

    fn write(&mut self, val: u8) {
        for pad in &mut self.joypads {
            pad.write(val);
        }
        for sig in &mut self.signatures {
            sig.write(val);
        }
    }
}

impl Clock for Input {
    fn clock(&mut self) -> usize {
        self.zapper.clock();
        if self.turbo_timer > 0 {
            self.turbo_timer -= 1;
        }
        if self.turbo_timer == 0 {
            // Roughly 20Hz
            self.turbo_timer += 89500;
            for pad in &mut self.joypads {
                if pad.button(JoypadBtnState::TURBO_A) {
                    let pressed = pad.button(JoypadBtnState::A);
                    pad.set_button(JoypadBtnState::A, !pressed);
                }
                if pad.button(JoypadBtnState::TURBO_B) {
                    let pressed = pad.button(JoypadBtnState::B);
                    pad.set_button(JoypadBtnState::B, !pressed);
                }
            }
        }
        1
    }
}

impl Reset for Input {
    fn reset(&mut self, kind: ResetKind) {
        for pad in &mut self.joypads {
            pad.reset(kind);
        }
        self.signatures[0] = Joypad::from_bytes(0b0000_1000);
        self.signatures[1] = Joypad::from_bytes(0b0000_0100);
        self.zapper.reset(kind);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JoypadBtn {
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
}

impl AsRef<str> for JoypadBtn {
    fn as_ref(&self) -> &str {
        match *self {
            JoypadBtn::A => "A",
            JoypadBtn::B => "B",
            JoypadBtn::Select => "Select",
            JoypadBtn::Start => "Start",
            JoypadBtn::Up => "Up",
            JoypadBtn::Down => "Down",
            JoypadBtn::Left => "Left",
            JoypadBtn::Right => "Right",
            JoypadBtn::TurboA => "A (Turbo)",
            JoypadBtn::TurboB => "B (Turbo)",
        }
    }
}

bitflags! {
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
    #[must_use]
    pub struct JoypadBtnState: u16 {
        const A = 0x01;
        const B = 0x02;
        const SELECT = 0x04;
        const START = 0x08;
        const UP = 0x10;
        const DOWN = 0x20;
        const LEFT = 0x40;
        const RIGHT = 0x80;
        const TURBO_A = 0x100;
        const TURBO_B = 0x200;
    }
}

impl From<JoypadBtn> for JoypadBtnState {
    fn from(button: JoypadBtn) -> Self {
        match button {
            JoypadBtn::A => Self::A,
            JoypadBtn::B => Self::B,
            JoypadBtn::Select => Self::SELECT,
            JoypadBtn::Start => Self::START,
            JoypadBtn::Up => Self::UP,
            JoypadBtn::Down => Self::DOWN,
            JoypadBtn::Left => Self::LEFT,
            JoypadBtn::Right => Self::RIGHT,
            JoypadBtn::TurboA => Self::TURBO_A,
            JoypadBtn::TurboB => Self::TURBO_B,
        }
    }
}

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Joypad {
    buttons: JoypadBtnState,
    index: u8,
    strobe: bool,
}

impl Joypad {
    pub const fn new() -> Self {
        Self {
            buttons: JoypadBtnState::from_bits_truncate(0),
            index: 0,
            strobe: false,
        }
    }

    #[must_use]
    pub const fn button(&self, button: JoypadBtnState) -> bool {
        self.buttons.contains(button)
    }

    pub fn set_button(&mut self, button: impl Into<JoypadBtnState>, pressed: bool) {
        self.buttons.set(button.into(), pressed);
    }

    pub fn from_bytes(val: u16) -> Self {
        Self {
            buttons: JoypadBtnState::from_bits_truncate(val),
            index: 0,
            strobe: false,
        }
    }

    #[must_use]
    pub fn read(&mut self) -> u8 {
        let val = self.peek();
        if !self.strobe && self.index < 8 {
            self.index += 1;
        }
        val
    }

    #[must_use]
    pub const fn peek(&self) -> u8 {
        if self.index < 8 {
            ((self.buttons.bits() as u8) & (1 << self.index)) >> self.index
        } else {
            0x01
        }
    }

    pub fn write(&mut self, val: u8) {
        let prev_strobe = self.strobe;
        self.strobe = val & 0x01 == 0x01;
        if prev_strobe && !self.strobe {
            self.index = 0;
        }
    }

    #[must_use]
    pub const fn index(&self) -> u8 {
        self.index
    }

    pub fn clear(&mut self) {
        self.buttons = JoypadBtnState::empty();
    }
}

impl Reset for Joypad {
    fn reset(&mut self, _kind: ResetKind) {
        self.buttons = JoypadBtnState::empty();
        self.index = 0;
        self.strobe = false;
    }
}

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Zapper {
    pub triggered: f32,
    pub x: u32,
    pub y: u32,
    pub radius: u32,
    pub connected: bool,
}

impl Zapper {
    #[must_use]
    pub const fn x(&self) -> u32 {
        self.x
    }

    #[must_use]
    pub const fn y(&self) -> u32 {
        self.y
    }

    pub fn trigger(&mut self) {
        if self.triggered <= 0.0 {
            // Zapper takes ~100ms to change to "released" after trigger is pulled
            self.triggered = Cpu::region_clock_rate(NesRegion::default()) / 10.0;
        }
    }

    pub fn aim(&mut self, x: u32, y: u32) {
        self.x = x;
        self.y = y;
    }

    pub fn clear(&mut self) {
        self.triggered = 0.0;
    }
}

impl Zapper {
    const fn new() -> Self {
        Self {
            triggered: 0.0,
            x: 0,
            y: 0,
            radius: 3,
            connected: false,
        }
    }

    #[must_use]
    fn read(&self, ppu: &Ppu) -> u8 {
        if self.connected {
            self.triggered() | self.light_sense(ppu)
        } else {
            0x00
        }
    }

    fn triggered(&self) -> u8 {
        if self.triggered > 0.0 {
            0x10
        } else {
            0x00
        }
    }

    fn light_sense(&self, ppu: &Ppu) -> u8 {
        let width = Ppu::WIDTH;
        let height = Ppu::HEIGHT;
        let scanline = ppu.scanline();
        let cycle = ppu.cycle();
        let min_y = self.y.saturating_sub(self.radius);
        let max_y = (self.y + self.radius).min(height - 1);
        let min_x = self.x.saturating_sub(self.radius);
        let max_x = (self.x + self.radius).min(width - 1);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let behind_ppu =
                    scanline >= y && (scanline - y) <= 20 && (scanline != y || cycle > x);
                if behind_ppu && ppu.pixel_brightness(x, y) >= 85 {
                    return 0x00;
                }
            }
        }
        0x08
    }
}

impl Clock for Zapper {
    fn clock(&mut self) -> usize {
        if self.triggered > 0.0 {
            self.triggered -= 1.0;
            1
        } else {
            0
        }
    }
}

impl Reset for Zapper {
    fn reset(&mut self, _kind: ResetKind) {
        self.triggered = 0.0;
    }
}
