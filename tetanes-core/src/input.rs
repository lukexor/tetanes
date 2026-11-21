//! [`Joypad`] and [`Zapper`] implementation.
//!
//! # Stability
//!
//! The methods here - [`Joypad::set_button`], [`Zapper::aim`] and the rest - are the input API and
//! are covered like any other. The *fields* are the emulation's internal wiring, public so that
//! embedders and debuggers can read them, and they track the implementation rather than the crate
//! version. See the crate-level [stability](crate#stability) note for the tier this belongs to and
//! why.

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

/// What is plugged into one of the two controller ports, read through $4016 or $4017.
///
/// Save states store this by variant name, so renaming a variant breaks them.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum PortDevice {
    /// A standard controller, or a [`FourPlayer`] adapter's view of two of them.
    #[default]
    Joypad,
    /// The [`Zapper`] light gun.
    Zapper,
    /// The NES [`PowerPad`] floor mat.
    PowerPad,
}

impl PortDevice {
    /// Every port device, for enumerating them in a UI.
    pub const fn as_slice() -> &'static [Self] {
        &[Self::Joypad, Self::Zapper, Self::PowerPad]
    }

    /// The device's stable string name, as used on the command line.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Joypad => "joypad",
            Self::Zapper => "zapper",
            Self::PowerPad => "power-pad",
        }
    }
}

impl AsRef<str> for PortDevice {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for PortDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Joypad => "Controller",
            Self::Zapper => "Zapper",
            Self::PowerPad => "Power Pad",
        };
        write!(f, "{s}")
    }
}

impl FromStr for PortDevice {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "joypad" => Ok(Self::Joypad),
            "zapper" => Ok(Self::Zapper),
            "power-pad" => Ok(Self::PowerPad),
            _ => Err("invalid PortDevice value. valid options: `joypad`, `zapper`, or `power-pad`"),
        }
    }
}

/// What is plugged into the Famicom's expansion port, read through $4016 and $4017 alongside the
/// controller ports.
///
/// Save states store this by variant name, so renaming a variant breaks them.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum ExpansionDevice {
    /// Nothing.
    #[default]
    None,
    /// The Family Trainer Mat, which reads the [`PowerPad`] buttons a row at a time.
    FamilyTrainer,
}

impl ExpansionDevice {
    /// Every expansion device, for enumerating them in a UI.
    pub const fn as_slice() -> &'static [Self] {
        &[Self::None, Self::FamilyTrainer]
    }

    /// The device's stable string name, as used on the command line.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::FamilyTrainer => "family-trainer",
        }
    }
}

impl AsRef<str> for ExpansionDevice {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ExpansionDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::None => "None",
            Self::FamilyTrainer => "Family Trainer",
        };
        write!(f, "{s}")
    }
}

impl FromStr for ExpansionDevice {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "family-trainer" => Ok(Self::FamilyTrainer),
            _ => Err("invalid ExpansionDevice value. valid options: `none` or `family-trainer`"),
        }
    }
}

/// The console's input hardware: what is plugged in, and the state of each device.
#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Input {
    /// The four controllers. Only the first two are reachable without an adapter.
    pub joypads: [Joypad; 4],
    /// The Four Score's two signature registers, shifted out after the pads.
    pub signatures: [Joypad; 2],
    /// The zapper, read when a port has [`PortDevice::Zapper`].
    pub zapper: Zapper,
    /// The floor mat, read when a port has [`PortDevice::PowerPad`] or the expansion port has
    /// [`ExpansionDevice::FamilyTrainer`].
    // Defaulted, as are `ports` and `expansion`, so a state saved before they existed still loads.
    #[serde(default)]
    pub power_pad: PowerPad,
    /// Frames left before the turbo buttons toggle.
    pub turbo_timer: u32,
    /// Which four-player adapter is plugged in.
    pub four_player: FourPlayer,
    /// What is plugged into ports 1 and 2.
    #[serde(default)]
    pub ports: [PortDevice; 2],
    /// What is plugged into the expansion port.
    #[serde(default)]
    pub expansion: ExpansionDevice,
}

impl Input {
    /// Creates input state timed for `region`.
    pub fn new(region: NesRegion) -> Self {
        Self {
            joypads: [Joypad::new(); 4],
            signatures: Self::signatures(),
            zapper: Zapper::new(region),
            power_pad: PowerPad::new(),
            turbo_timer: 30,
            four_player: FourPlayer::default(),
            ports: [PortDevice::Joypad; 2],
            expansion: ExpansionDevice::None,
        }
    }

    /// The Four Score's signature registers before any of them is read.
    const fn signatures() -> [Joypad; 2] {
        // Signature bits are reversed so they can shift right
        [
            Joypad::from_bytes(0b0000_1000),
            Joypad::from_bytes(0b0000_0100),
        ]
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

    /// Returns what is plugged into `port`. Ports three and four always hold a controller.
    pub const fn port_device(&self, port: Player) -> PortDevice {
        match port {
            Player::One => self.ports[0],
            Player::Two => self.ports[1],
            Player::Three | Player::Four => PortDevice::Joypad,
        }
    }

    /// Plugs `device` into `port`, releasing that device's buttons if it changes what is plugged
    /// in. Ports three and four only take a controller, so they are ignored.
    ///
    /// Selecting what is already selected changes nothing, for the reason given on
    /// [`Input::set_four_player`].
    pub fn set_port_device(&mut self, port: Player, device: PortDevice) {
        let slot = match port {
            Player::One => &mut self.ports[0],
            Player::Two => &mut self.ports[1],
            Player::Three | Player::Four => return,
        };
        if *slot == device {
            return;
        }
        *slot = device;
        self.clear_device(device);
    }

    /// Plugs `device` into the expansion port, releasing its buttons if that changes what is
    /// plugged in.
    pub fn set_expansion_device(&mut self, device: ExpansionDevice) {
        if self.expansion == device {
            return;
        }
        self.expansion = device;
        if device == ExpansionDevice::FamilyTrainer {
            self.power_pad.reset(ResetKind::Hard);
        }
    }

    /// Resets the state of a device that was just plugged in.
    const fn clear_device(&mut self, device: PortDevice) {
        match device {
            PortDevice::Joypad => (),
            PortDevice::Zapper => self.zapper.reset(ResetKind::Hard),
            PortDevice::PowerPad => self.power_pad.reset(ResetKind::Hard),
        }
    }

    /// Selects the four-player adapter, clearing all input state if that changes what is plugged
    /// in.
    ///
    /// Selecting what is already selected changes nothing, and must not clear anything: restoring
    /// a state carries the session's adapter across every time, and rewind does it once per
    /// display frame. Resetting unconditionally there released any button the player was holding.
    pub fn set_four_player(&mut self, four_player: FourPlayer) {
        if self.four_player == four_player {
            return;
        }
        self.four_player = four_player;
        self.reset(ResetKind::Hard);
    }

    /// Releases every button, the mat's included, and clears the zapper trigger.
    pub fn clear(&mut self) {
        for pad in &mut self.joypads {
            pad.clear();
        }
        self.zapper.clear();
        self.power_pad.clear();
    }

    /// Takes the buttons, trigger and aim from `live`, keeping every shift register's position.
    ///
    /// For restoring a state under a player who is still holding the controls. A key held in the
    /// state but not now would otherwise stay pressed, since the release it needs already came.
    pub const fn keep_held(&mut self, live: &Self) {
        let mut i = 0;
        while i < self.joypads.len() {
            self.joypads[i].buttons = live.joypads[i].buttons;
            i += 1;
        }
        self.power_pad.buttons = live.power_pad.buttons;
        self.zapper.triggered = live.zapper.triggered;
        self.zapper.x = live.zapper.x;
        self.zapper.y = live.zapper.y;
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
                if self.joypads[player].index < 8 {
                    self.joypads[player].read()
                } else if self.joypads[player + 2].index < 8 {
                    self.joypads[player + 2].read()
                } else if self.signatures[player].index < 8 {
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
                if self.joypads[player].index < 8 {
                    self.joypads[player].peek()
                } else if self.joypads[player + 2].index < 8 {
                    self.joypads[player + 2].peek()
                } else if self.signatures[player].index < 8 {
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
        self.power_pad.write(val);
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
        self.signatures = Self::signatures();
        self.zapper.reset(kind);
        self.power_pad.reset(kind);
    }
}

/// The console's view of the input ports. The zapper senses light from the PPU's output, so
/// reading a port takes the [`Bus`].
impl Bus {
    /// Reads $4016/$4017 for one port, shifting the device's register on.
    pub(crate) fn input_read(&mut self, player: Player) -> u8 {
        let port = match self.input.port_device(player) {
            PortDevice::Joypad => self.input.read_port(player),
            PortDevice::Zapper => self.input.zapper.read(&self.ppu),
            PortDevice::PowerPad => self.input.power_pad.read_serial(),
        };
        port | self.expansion_peek(player) | 0x40
    }

    /// Reads $4016/$4017 without shifting.
    #[must_use]
    pub(crate) fn input_peek(&self, player: Player) -> u8 {
        let port = match self.input.port_device(player) {
            PortDevice::Joypad => self.input.peek_port(player),
            PortDevice::Zapper => self.input.zapper.read(&self.ppu),
            PortDevice::PowerPad => self.input.power_pad.peek_serial(),
        };
        port | self.expansion_peek(player) | 0x40
    }

    /// The expansion port's bits of $4016/$4017. No expansion device shifts on read.
    const fn expansion_peek(&self, player: Player) -> u8 {
        match (self.input.expansion, player) {
            (ExpansionDevice::FamilyTrainer, Player::Two) => self.input.power_pad.peek_rows(),
            _ => 0x00,
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
    /// The aim's X position in screen pixels.
    #[serde(skip)] // Don't save zapper position
    pub x: u16,
    /// The aim's Y position in screen pixels.
    #[serde(skip)] // Don't save zapper position
    pub y: u16,
    /// Radius in pixels around the aim point sampled for light.
    pub radius: u16,
}

impl Zapper {
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
        }
    }

    /// Bit 4 is the trigger and bit 3 is the light sense, 0 when light is seen.
    #[must_use]
    fn read(&self, ppu: &Ppu) -> u8 {
        self.triggered() | self.light_sense(ppu)
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

/// A 12-button floor mat. A port reads it as the NES Power Pad ([`PortDevice::PowerPad`]), and
/// the expansion port reads it as the Famicom Family Trainer Mat
/// ([`ExpansionDevice::FamilyTrainer`]).
///
/// Buttons are numbered 1 to 12, left to right and top to bottom, as printed on the Power Pad's
/// side B. A game made for side A sees the same numbers, with the mat flipped over.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct PowerPad {
    /// Which buttons are held, button `n` at bit `n - 1`.
    pub buttons: u16,
    /// How far the Power Pad's shift registers have been read.
    pub index: u8,
    /// Whether $4016 bit 0 is holding the Power Pad's registers in reload.
    pub strobe: bool,
    /// The Family Trainer's row select from $4016 bits 0-2. A clear bit includes its row.
    pub rows: u8,
}

impl Default for PowerPad {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerPad {
    /// The number of buttons on the mat.
    pub const BUTTONS: u8 = 12;

    /// Buttons shifted out of the Power Pad's $4017 bit 3, in read order.
    const D3_ORDER: [u8; 8] = [2, 1, 5, 9, 6, 10, 11, 7];
    /// Buttons shifted out of the Power Pad's $4017 bit 4, in read order. Its last four reads
    /// return 1.
    const D4_ORDER: [u8; 4] = [4, 3, 12, 8];

    /// Creates a mat with nothing pressed.
    pub const fn new() -> Self {
        Self {
            buttons: 0,
            index: 0,
            strobe: false,
            rows: 0x07,
        }
    }

    /// Whether button `button` (1 to 12) is held.
    #[must_use]
    pub const fn button(&self, button: u8) -> bool {
        matches!(button, 1..=Self::BUTTONS) && self.buttons & (1 << (button - 1)) != 0
    }

    /// Presses or releases button `button` (1 to 12). Any other number is ignored.
    pub const fn set_button(&mut self, button: u8, pressed: bool) {
        if !matches!(button, 1..=Self::BUTTONS) {
            return;
        }
        let bit = 1 << (button - 1);
        if pressed {
            self.buttons |= bit;
        } else {
            self.buttons &= !bit;
        }
    }

    /// Reads the Power Pad's bits of its port, shifting its registers on.
    #[must_use]
    pub const fn read_serial(&mut self) -> u8 {
        let val = self.peek_serial();
        if !self.strobe && self.index < 8 {
            self.index += 1;
        }
        val
    }

    /// The Power Pad's bits 3 and 4: the next button from each shift register, 1 when pressed.
    #[must_use]
    pub const fn peek_serial(&self) -> u8 {
        let i = self.index as usize;
        if i >= 8 {
            return 0x18;
        }
        let d3 = self.button(Self::D3_ORDER[i]);
        let d4 = i >= Self::D4_ORDER.len() || self.button(Self::D4_ORDER[i]);
        ((d4 as u8) << 4) | ((d3 as u8) << 3)
    }

    /// The Family Trainer's bits 1 to 4 of $4017: one per column, left-most column at bit 4, 0
    /// when a button in a selected row is pressed.
    #[must_use]
    pub const fn peek_rows(&self) -> u8 {
        // Bit 2 selects the top row (buttons 1-4), bit 1 the middle and bit 0 the bottom.
        let mut columns = 0;
        let mut row = 0;
        while row < 3 {
            if self.rows & (0x04 >> row) == 0 {
                columns |= (self.buttons >> (row * 4)) & 0x0F;
            }
            row += 1;
        }
        let mut val = 0x1E;
        let mut column = 0;
        while column < 4 {
            if columns & (1 << column) != 0 {
                val &= !(0x10 >> column);
            }
            column += 1;
        }
        val
    }

    /// Writes $4016. Bit 0 is the Power Pad's strobe, and bits 0-2 are the Family Trainer's row
    /// select.
    pub const fn write(&mut self, val: u8) {
        self.strobe = val & 0x01 == 0x01;
        if self.strobe {
            self.index = 0;
        }
        self.rows = val & 0x07;
    }

    /// Releases every button.
    pub const fn clear(&mut self) {
        self.buttons = 0;
    }

    /// Resets the mat, releasing every button.
    pub const fn reset(&mut self, _kind: ResetKind) {
        self.buttons = 0;
        self.index = 0;
        self.strobe = false;
        self.rows = 0x07;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(held: &[u8]) -> PowerPad {
        let mut pad = PowerPad::new();
        for &button in held {
            pad.set_button(button, true);
        }
        pad
    }

    /// Strobes, then reads the eight (bit 3, bit 4) pairs a game shifts out.
    fn shift_out(pad: &mut PowerPad) -> Vec<(bool, bool)> {
        pad.write(1);
        pad.write(0);
        (0..8)
            .map(|_| {
                let val = pad.read_serial();
                (val & 0x08 != 0, val & 0x10 != 0)
            })
            .collect()
    }

    #[test]
    fn power_pad_shifts_buttons_in_hardware_order() {
        for button in 1..=PowerPad::BUTTONS {
            let mut pad = pad(&[button]);
            let bits = shift_out(&mut pad);
            let d3 = PowerPad::D3_ORDER.iter().position(|&b| b == button);
            let d4 = PowerPad::D4_ORDER.iter().position(|&b| b == button);
            for (i, &(l, h)) in bits.iter().enumerate() {
                assert_eq!(l, d3 == Some(i), "button {button}, bit 3, read {i}");
                assert_eq!(
                    h,
                    d4 == Some(i) || i >= 4,
                    "button {button}, bit 4, read {i}"
                );
            }
        }
    }

    #[test]
    fn power_pad_reads_one_after_eight() {
        let mut pad = pad(&[]);
        let _ = shift_out(&mut pad);
        assert_eq!(pad.read_serial(), 0x18);
    }

    #[test]
    fn power_pad_strobe_high_repeats_first_bit() {
        let mut pad = pad(&[2]);
        pad.write(1);
        assert_eq!(pad.read_serial() & 0x08, 0x08);
        assert_eq!(pad.read_serial() & 0x08, 0x08);
    }

    #[test]
    fn family_trainer_scans_selected_rows() {
        // Button 6 is middle row, second column, so bit 3.
        let mut pad = pad(&[6]);
        pad.write(0b101);
        assert_eq!(pad.peek_rows(), 0x1E & !0x08, "middle row selected");
        pad.write(0b011);
        assert_eq!(pad.peek_rows(), 0x1E, "top row selected");
        pad.write(0b111);
        assert_eq!(pad.peek_rows(), 0x1E, "no row selected");
    }

    #[test]
    fn family_trainer_maps_columns_to_bits() {
        for (button, bit) in [
            (1, 0x10),
            (2, 0x08),
            (3, 0x04),
            (4, 0x02),
            (12, 0x02),
            (9, 0x10),
        ] {
            let mut pad = pad(&[button]);
            pad.write(0b000);
            assert_eq!(pad.peek_rows(), 0x1E & !bit, "button {button}");
        }
    }

    #[test]
    fn set_button_ignores_out_of_range() {
        let mut pad = PowerPad::new();
        pad.set_button(0, true);
        pad.set_button(13, true);
        assert_eq!(pad.buttons, 0);
    }

    #[test]
    fn a_port_device_replaces_the_controller() {
        for port in [Player::One, Player::Two] {
            let mut bus = Bus::default();
            bus.input.joypad_mut(port).set_button(JoypadBtn::A, true);
            bus.input.set_port_device(port, PortDevice::PowerPad);
            bus.input.power_pad.set_button(2, true);
            bus.input.write(1);
            bus.input.write(0);
            assert_eq!(bus.input_read(port), 0x40 | 0x08, "{port:?}");
        }
    }

    #[test]
    fn the_family_trainer_shares_port_two_with_the_controller() {
        let mut bus = Bus::default();
        bus.input.joypads[1].set_button(JoypadBtn::A, true);
        bus.input
            .set_expansion_device(ExpansionDevice::FamilyTrainer);
        bus.input.write(1);
        assert_eq!(bus.input_read(Player::Two), 0x40 | 0x1E | 0x01);
        assert_eq!(bus.input_read(Player::One), 0x40);
    }

    #[test]
    fn ports_three_and_four_only_take_a_controller() {
        let mut input = Input::new(NesRegion::Ntsc);
        input.set_port_device(Player::Three, PortDevice::Zapper);
        assert_eq!(input.port_device(Player::Three), PortDevice::Joypad);
        assert_eq!(input.ports, [PortDevice::Joypad; 2]);
    }
}
