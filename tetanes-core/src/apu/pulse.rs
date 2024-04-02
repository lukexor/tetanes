use crate::{
    apu::{envelope::Envelope, length_counter::LengthCounter, sweep::Sweep},
    common::{Clock, Reset, ResetKind, Sample},
};
use serde::{Deserialize, Serialize};

/// Pulse Channel selection.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum PulseChannel {
    One,
    Two,
}

/// APU Pulse Channel provides square wave generation.
///
/// See: <https://www.nesdev.org/wiki/APU_Pulse>
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Pulse {
    pub channel: PulseChannel,
    pub timer: u16,
    pub period: u16,
    pub duty: u8,       // Select row in DUTY_TABLE
    pub duty_cycle: u8, // Select column in DUTY_TABLE
    pub length: LengthCounter,
    pub envelope: Envelope,
    pub sweep: Sweep,
    pub force_silent: bool,
}

impl Default for Pulse {
    fn default() -> Self {
        Self::new(PulseChannel::One)
    }
}

impl Pulse {
    const DUTY_TABLE: [[u8; 8]; 4] = [
        [0, 0, 0, 0, 0, 0, 0, 1],
        [0, 0, 0, 0, 0, 0, 1, 1],
        [0, 0, 0, 0, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 0, 0],
    ];

    pub const fn new(channel: PulseChannel) -> Self {
        Self {
            channel,
            timer: 0,
            period: 0,
            duty: 0u8,
            duty_cycle: 0,
            length: LengthCounter::new(),
            envelope: Envelope::new(),
            sweep: Sweep::new(channel),
            force_silent: false,
        }
    }

    fn is_muted(&self) -> bool {
        self.sweep.is_muted()
    }

    #[must_use]
    pub const fn silent(&self) -> bool {
        self.force_silent
    }

    pub fn toggle_silent(&mut self) {
        self.force_silent = !self.force_silent;
    }

    #[must_use]
    pub const fn length_counter(&self) -> u8 {
        self.length.counter
    }

    pub fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.envelope.clock();
        self.length.clock();
        if self.sweep.clock() > 0 {
            self.period = self.sweep.period + 1;
        }
    }

    /// $4000/$4004 Pulse control
    pub fn write_ctrl(&mut self, val: u8) {
        self.length.write_ctrl((val & 0x20) == 0x20); // !D5
        self.envelope.write_ctrl(val);
        self.duty = (val & 0xC0) >> 6;
    }

    /// $4001/$4005 Pulse sweep
    pub fn write_sweep(&mut self, val: u8) {
        self.sweep.write(val);
    }

    /// $4002/$4006 Pulse timer lo
    pub fn write_timer_lo(&mut self, val: u8) {
        self.sweep.write_timer_lo(val);
        self.period = self.sweep.period + 1;
    }

    /// $4003/$4007 Pulse timer hi
    pub fn write_timer_hi(&mut self, val: u8) {
        self.length.write(val);
        self.sweep.write_timer_hi(val & 0x07);
        self.period = self.sweep.period + 1;
        self.duty_cycle = 0;
        self.envelope.restart();
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.length.set_enabled(enabled);
    }

    fn volume(&self) -> u8 {
        if self.length.counter > 0 {
            self.envelope.volume()
        } else {
            0
        }
    }
}

impl Sample for Pulse {
    #[must_use]
    fn output(&self) -> f32 {
        if self.is_muted() || self.silent() {
            0.0
        } else {
            f32::from(
                Self::DUTY_TABLE[self.duty as usize][self.duty_cycle as usize] * self.volume(),
            )
        }
    }
}

impl Clock for Pulse {
    fn clock(&mut self) -> usize {
        if self.timer > 0 {
            self.timer -= 1;
        } else {
            self.duty_cycle = self.duty_cycle.wrapping_sub(1) & 0x07;
            self.timer = self.period;
        }

        1
    }
}

impl Reset for Pulse {
    fn reset(&mut self, kind: ResetKind) {
        self.length.reset(kind);
        self.envelope.reset(kind);
        self.sweep.reset(kind);
        self.duty = 0;
        self.duty_cycle = 0;
    }
}
