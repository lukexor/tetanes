//! [`Joypad`] and [`Zapper`] implementation.
//!
//! # Stability
//!
//! The methods here - [`Joypad::set_button`], [`Zapper::aim`] and the rest - are the input API and
//! are covered like any other. The *fields* are the emulation's internal wiring, public so that
//! embedders and debuggers can read them, and they track the implementation rather than the crate
//! version.

use crate::{
    bus::Bus,
    common::{NesRegion, ResetKind},
    cpu::Cpu,
    ppu::{Ppu, size},
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;
use tracing::trace;

/// Error parsing a [`Player`] from a string.
#[derive(Error, Debug)]
#[must_use]
#[error("failed to parse `Player`")]
pub struct ParsePlayerError;

/// Which controller port an input belongs to.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Player {
    /// Controller port 1.
    #[default]
    One,
    /// Controller port 2.
    Two,
    /// Port 3, reachable only through a [`FourPlayer`] adapter.
    Three,
    /// Port 4, reachable only through a [`FourPlayer`] adapter.
    Four,
}

impl std::fmt::Display for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::One => "One",
            Self::Two => "Two",
            Self::Three => "Three",
            Self::Four => "Four",
        };
        write!(f, "{s}")
    }
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

impl TryFrom<usize> for Player {
    type Error = ParsePlayerError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::One),
            1 => Ok(Self::Two),
            2 => Ok(Self::Three),
            3 => Ok(Self::Four),
            _ => Err(ParsePlayerError),
        }
    }
}

/// Which four-player adapter, if any, is plugged in. The two wire their extra pads up
/// differently, so a game written for one does not see the other.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum FourPlayer {
    /// No adapter: two controllers.
    #[default]
    Disabled,
    /// NES Four Score, which shifts all four pads plus a signature out of the two ports.
    FourScore,
    /// Famicom Four Players Adapter, which reports the extra pads through $4016/$4017 bit 1.
    Satellite,
}

impl FourPlayer {
    /// Every adapter setting, for enumerating them in a UI.
    pub const fn as_slice() -> &'static [Self] {
        &[Self::Disabled, Self::FourScore, Self::Satellite]
    }

    /// The setting's stable string name, as used in config files.
    pub const fn as_str(&self) -> &'static str {
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
        let s = match self {
            Self::Disabled => "Disabled",
            Self::FourScore => "FourScore",
            Self::Satellite => "Satellite",
        };
        write!(f, "{s}")
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

/// The console's input hardware: the controller ports, the zapper and the four-player adapter.
#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Input {
    /// The four controller ports; only the first two exist without an adapter.
    pub joypads: [Joypad; 4],
    /// The Four Score's two signature registers, shifted out after the pads.
    pub signatures: [Joypad; 2],
    /// The zapper, whether or not it is plugged in.
    pub zapper: Zapper,
    /// Frames left before the turbo buttons toggle.
    pub turbo_timer: u32,
    /// Which four-player adapter is plugged in.
    pub four_player: FourPlayer,
}

impl Input {
    /// Creates input state timed for `region`.
    pub fn new(region: NesRegion) -> Self {
        Self {
            joypads: [Joypad::new(); 4],
            // Signature bits are reversed so they can shift right
            signatures: [
                Joypad::from_bytes(0b0000_1000),
                Joypad::from_bytes(0b0000_0100),
            ],
            zapper: Zapper::new(region),
            turbo_timer: 30,
            four_player: FourPlayer::default(),
        }
    }

    /// Borrows one port's controller.
    pub const fn joypad(&self, player: Player) -> &Joypad {
        &self.joypads[player as usize]
    }

    /// Mutably borrows one port's controller, which is how buttons are set.
    pub const fn joypad_mut(&mut self, player: Player) -> &mut Joypad {
        &mut self.joypads[player as usize]
    }

    /// Sets the region, which re-times the zapper's trigger release.
    pub fn set_region(&mut self, region: NesRegion) {
        self.zapper.trigger_release_delay = Cpu::region_clock_rate(region) / 10.0;
    }

    /// Allows opposing D-Pad directions to be held at once. The hardware permits it and a few
    /// games rely on it, but most were never tested against it.
    pub fn set_concurrent_dpad(&mut self, enabled: bool) {
        self.joypads
            .iter_mut()
            .for_each(|pad| pad.concurrent_dpad = enabled);
    }

    /// Plugs the zapper into port 2, or unplugs it.
    pub const fn connect_zapper(&mut self, connected: bool) {
        self.zapper.connected = connected;
    }

    /// Selects the four-player adapter, clearing all input state.
    pub fn set_four_player(&mut self, four_player: FourPlayer) {
        self.four_player = four_player;
        self.reset(ResetKind::Hard);
    }

    /// Releases every button and clears the zapper trigger.
    pub fn clear(&mut self) {
        for pad in &mut self.joypads {
            pad.clear();
        }
        self.zapper.clear();
    }

    /// Reads one controller port, shifting its register on.
    ///
    /// The joypads and four-score signatures only. The zapper shares $4017 with port two;
    /// `Bus::input_read` composes the two.
    pub fn read_port(&mut self, player: Player) -> u8 {
        // Read $4016/$4017 D0 8x for controller #1/#2.
        // Read $4016/$4017 D0 8x for controller #3/#4.
        // Read $4016/$4017 D0 8x for signature: 0b00010000/0b00100000
        let player = player as usize;
        assert!(player < 4);
        match self.four_player {
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
        }
    }

    /// Reads one controller port without shifting. See [`Input::read_port`].
    pub fn peek_port(&self, player: Player) -> u8 {
        // Read $4016/$4017 D0 8x for controller #1/#2.
        // Read $4016/$4017 D0 8x for controller #3/#4.
        // Read $4016/$4017 D0 8x for signature: 0b00010000/0b00100000
        let player = player as usize;
        assert!(player < 4);
        match self.four_player {
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
        }
    }

    /// Writes $4016, whose bit 0 is the strobe that reloads every controller register.
    pub fn write(&mut self, val: u8) {
        for pad in &mut self.joypads {
            pad.write(val);
        }
        for sig in &mut self.signatures {
            sig.write(val);
        }
    }

    /// Clocks the turbo timer and the zapper's trigger release.
    pub fn clock(&mut self) {
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
    }

    /// Resets input state.
    pub fn reset(&mut self, kind: ResetKind) {
        for pad in &mut self.joypads {
            pad.reset(kind);
        }
        self.signatures[0] = Joypad::from_bytes(0b0000_1000);
        self.signatures[1] = Joypad::from_bytes(0b0000_0100);
        self.zapper.reset(kind);
    }
}

/// The console's view of the input ports: $4017 is a controller port *and* the zapper, which
/// senses light from the PPU's output, so reading one takes the [`Bus`].
impl Bus {
    /// Reads $4016/$4017 for one port, shifting the controller's register on.
    pub(crate) fn input_read(&mut self, player: Player) -> u8 {
        self.zapper_sense(player) | self.input.read_port(player) | 0x40
    }

    /// Reads $4016/$4017 without shifting.
    #[must_use]
    pub(crate) fn input_peek(&self, player: Player) -> u8 {
        self.zapper_sense(player) | self.input.peek_port(player) | 0x40
    }

    /// The zapper's trigger and light-sense bits, which only share $4017 with port two.
    fn zapper_sense(&self, player: Player) -> u8 {
        if player == Player::Two {
            self.input.zapper.read(&self.ppu)
        } else {
            0x00
        }
    }
}

/// A single controller button, including the two synthetic turbo buttons.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// The set of controller buttons currently held, as one bit each.
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
    #[must_use]
    pub struct JoypadBtnState: u16 {
        /// A button.
        const A = 0x01;
        /// B button.
        const B = 0x02;
        /// Select button.
        const SELECT = 0x04;
        /// Start button.
        const START = 0x08;
        /// Up on the D-Pad.
        const UP = 0x10;
        /// Down on the D-Pad.
        const DOWN = 0x20;
        /// Left on the D-Pad.
        const LEFT = 0x40;
        /// Right on the D-Pad.
        const RIGHT = 0x80;
        /// Synthetic: A, auto-fired by the turbo timer.
        const TURBO_A = 0x100;
        /// Synthetic: B, auto-fired by the turbo timer.
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

/// A standard NES controller: eight buttons behind a shift register.
#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Joypad {
    /// Which buttons are currently held.
    pub buttons: JoypadBtnState,
    /// Whether opposing D-Pad directions may be held at once.
    pub concurrent_dpad: bool,
    /// How far the shift register has been read.
    pub index: u8,
    /// Whether $4016 bit 0 is holding the register in reload.
    pub strobe: bool,
}

impl Joypad {
    /// Creates a controller with nothing pressed.
    pub const fn new() -> Self {
        Self {
            buttons: JoypadBtnState::empty(),
            concurrent_dpad: false,
            index: 0,
            strobe: false,
        }
    }

    /// Whether a button is currently held.
    #[must_use]
    pub const fn button(&self, button: JoypadBtnState) -> bool {
        self.buttons.contains(button)
    }

    /// Presses or releases a button, applying the D-Pad rule from `concurrent_dpad`.
    pub fn set_button(&mut self, button: impl Into<JoypadBtnState>, pressed: bool) {
        let button = button.into();
        let prevent_concurrent_dpad = pressed && !self.concurrent_dpad;
        if let Some(button) = match button {
            JoypadBtnState::LEFT if prevent_concurrent_dpad => Some(JoypadBtnState::RIGHT),
            JoypadBtnState::RIGHT if prevent_concurrent_dpad => Some(JoypadBtnState::LEFT),
            JoypadBtnState::UP if prevent_concurrent_dpad => Some(JoypadBtnState::DOWN),
            JoypadBtnState::DOWN if prevent_concurrent_dpad => Some(JoypadBtnState::UP),
            JoypadBtnState::TURBO_A if !pressed => Some(JoypadBtnState::A),
            JoypadBtnState::TURBO_B if !pressed => Some(JoypadBtnState::B),
            _ => None,
        } {
            self.buttons.set(button, false);
        }
        self.buttons.set(button, pressed);
    }

    /// Builds a controller whose held buttons come from a raw [`JoypadBtnState`] bit pattern.
    pub const fn from_bytes(val: u16) -> Self {
        Self {
            buttons: JoypadBtnState::from_bits_truncate(val),
            concurrent_dpad: false,
            index: 0,
            strobe: false,
        }
    }

    /// Shifts the next button out of the register.
    #[must_use]
    pub const fn read(&mut self) -> u8 {
        let val = self.peek();
        if !self.strobe && self.index < 8 {
            self.index += 1;
        }
        val
    }

    /// Returns the next button without shifting.
    #[must_use]
    pub const fn peek(&self) -> u8 {
        if self.index < 8 {
            ((self.buttons.bits() as u8) & (1 << self.index)) >> self.index
        } else {
            0x01
        }
    }

    /// Writes the strobe line; while it is high the register reloads continuously.
    pub const fn write(&mut self, val: u8) {
        let prev_strobe = self.strobe;
        self.strobe = val & 0x01 == 0x01;
        if prev_strobe && !self.strobe {
            self.index = 0;
        }
    }

    /// How far the shift register has been read.
    #[must_use]
    pub const fn index(&self) -> u8 {
        self.index
    }

    /// Releases every button.
    pub const fn clear(&mut self) {
        self.buttons = JoypadBtnState::empty();
    }

    /// Resets the shift register.
    pub const fn reset(&mut self, _kind: ResetKind) {
        self.buttons = JoypadBtnState::empty();
        self.index = 0;
        self.strobe = false;
    }
}

/// The NES Zapper light gun, which reports whether its trigger is pulled and whether the pixel
/// it is aimed at is lit.
#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Zapper {
    /// Seconds left before the trigger releases itself; 0 when not pulled.
    #[serde(skip)] // Don't save triggered state
    pub triggered: f32,
    /// How long a pull lasts, in seconds.
    pub trigger_release_delay: f32,
    /// Aim position in screen pixels.
    #[serde(skip)] // Don't save zapper position
    pub x: u16,
    /// Aim position in screen pixels.
    #[serde(skip)] // Don't save zapper position
    pub y: u16,
    /// Radius in pixels around the aim point sampled for light.
    pub radius: u16,
    /// Whether the zapper is plugged in.
    pub connected: bool,
}

impl Zapper {
    /// The aim's X position in screen pixels.
    #[inline(always)]
    #[must_use]
    pub const fn x(&self) -> u16 {
        self.x
    }

    /// The aim's Y position in screen pixels.
    #[inline(always)]
    #[must_use]
    pub const fn y(&self) -> u16 {
        self.y
    }

    /// Pulls the trigger, which releases itself after `trigger_release_delay`.
    #[inline(always)]
    pub fn trigger(&mut self) {
        if self.triggered <= 0.0 {
            self.triggered = self.trigger_release_delay;
        }
    }

    /// Points the zapper at a screen pixel.
    #[inline(always)]
    pub const fn aim(&mut self, x: u16, y: u16) {
        self.x = x;
        self.y = y;
    }

    /// Releases the trigger.
    pub const fn clear(&mut self) {
        self.triggered = 0.0;
    }
    fn new(region: NesRegion) -> Self {
        Self {
            triggered: 0.0,
            // Zapper takes ~100ms to change to "released" after trigger is pulled
            trigger_release_delay: Cpu::region_clock_rate(region) / 10.0,
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
        if self.triggered > 0.0 { 0x10 } else { 0x00 }
    }

    fn light_sense(&self, ppu: &Ppu) -> u8 {
        let width = size::WIDTH;
        let height = size::HEIGHT;
        let scanline = ppu.scanline;
        let cycle = ppu.cycle;
        let min_y = self.y.saturating_sub(self.radius);
        let max_y = (self.y + self.radius).min(height - 1);
        let min_x = self.x.saturating_sub(self.radius);
        let max_x = (self.x + self.radius).min(width - 1);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let behind_ppu =
                    scanline >= y && (scanline - y) <= 20 && (scanline != y || cycle > x);
                let brightness = ppu.pixel_brightness(x, y);
                if behind_ppu && brightness >= 85 {
                    trace!("zapper light: {brightness}");
                    return 0x00;
                }
            }
        }
        0x08
    }

    /// Counts the trigger's self-release down by one CPU cycle.
    pub fn clock(&mut self) {
        if self.triggered > 0.0 {
            self.triggered -= 1.0;
        }
    }

    /// Resets the zapper, releasing the trigger.
    pub const fn reset(&mut self, _kind: ResetKind) {
        self.triggered = 0.0;
    }
}
