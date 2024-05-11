//! APU Pulse Channel implementation.
//!
//! See: <https://www.nesdev.org/wiki/APU_Pulse>

use crate::{
    apu::{
        envelope::Envelope,
        length_counter::LengthCounter,
        timer::{Timer, TimerCycle},
        Channel,
    },
    common::{Clock, Reset, ResetKind, Sample},
};
use serde::{Deserialize, Serialize};

/// Pulse Channel output frequency. Supports MMC5 being able to pulse at ultrasonic frequencies.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum OutputFreq {
    Default,
    Ultrasonic,
}

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
    pub real_period: usize,
    pub timer: Timer,
    pub duty: u8,       // Select row in DUTY_TABLE
    pub duty_cycle: u8, // Select column in DUTY_TABLE
    pub length: LengthCounter,
    pub envelope: Envelope,
    pub sweep: Sweep,
    pub force_silent: bool,
    pub output_freq: OutputFreq,
}

impl Default for Pulse {
    fn default() -> Self {
        Self::new(PulseChannel::One, OutputFreq::Default)
    }
}

impl Pulse {
    const DUTY_TABLE: [[u8; 8]; 4] = [
        [0, 0, 0, 0, 0, 0, 0, 1],
        [0, 0, 0, 0, 0, 0, 1, 1],
        [0, 0, 0, 0, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 0, 0],
    ];

    pub const fn new(channel: PulseChannel, output_freq: OutputFreq) -> Self {
        Self {
            channel,
            real_period: 0,
            timer: Timer::new(0),
            duty: 0u8,
            duty_cycle: 0,
            length: LengthCounter::new(match channel {
                PulseChannel::One => Channel::Pulse1,
                PulseChannel::Two => Channel::Pulse2,
            }),
            envelope: Envelope::new(),
            sweep: Sweep::new(channel),
            force_silent: false,
            output_freq,
        }
    }

    #[inline]
    pub fn is_muted(&self) -> bool {
        // MMC5 doesn't mute at ultasonic frequencies
        self.output_freq == OutputFreq::Default
            && (self.real_period < 8 || (!self.sweep.negate && self.sweep.target_period > 0x7FF))
            || self.silent()
    }

    #[must_use]
    pub const fn silent(&self) -> bool {
        self.force_silent
    }

    pub fn set_silent(&mut self, silent: bool) {
        self.force_silent = silent;
    }

    fn update_target_period(&mut self) {
        let delta = self.real_period >> self.sweep.shift;
        if self.sweep.negate {
            self.sweep.target_period = self.real_period - delta;
            if let PulseChannel::One = self.channel {
                self.sweep.target_period = self.sweep.target_period.wrapping_sub(1);
            }
        } else {
            self.sweep.target_period = self.real_period + delta;
        }
    }

    fn set_period(&mut self, period: usize) {
        self.real_period = period;
        self.timer.period = (period * 2) + 1;
        self.update_target_period();
    }

    fn clock_sweep(&mut self) {
        self.sweep.divider = self.sweep.divider.wrapping_sub(1);
        if self.sweep.divider == 0 {
            if self.sweep.shift > 0
                && self.sweep.enabled
                && self.real_period >= 8
                && self.sweep.target_period <= 0x7FF
            {
                self.set_period(self.sweep.target_period);
            }
            self.sweep.divider = self.sweep.period;
        }

        if self.sweep.reload {
            self.sweep.divider = self.sweep.period;
            self.sweep.reload = false;
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.clock_quarter_frame();
        self.length.clock();
        self.clock_sweep();
    }

    /// $4000/$4004 Pulse control
    pub fn write_ctrl(&mut self, val: u8) {
        self.length.write_ctrl((val & 0x20) == 0x20); // !D5
        self.envelope.write_ctrl(val);
        self.duty = (val & 0xC0) >> 6;
    }

    /// $4001/$4005 Pulse sweep
    pub fn write_sweep(&mut self, val: u8) {
        self.sweep.enabled = (val & 0x80) == 0x80;
        self.sweep.negate = (val & 0x08) == 0x08;
        self.sweep.period = ((val & 0x70) >> 4) + 1;
        self.sweep.shift = val & 0x07;
        self.update_target_period();
        self.sweep.reload = true;
    }

    /// $4002/$4006 Pulse timer lo
    pub fn write_timer_lo(&mut self, val: u8) {
        self.set_period(self.real_period & 0x0700 | usize::from(val));
    }

    /// $4003/$4007 Pulse timer hi
    pub fn write_timer_hi(&mut self, val: u8) {
        self.length.write(val >> 3);
        self.set_period(self.real_period & 0xFF | (usize::from(val & 0x07) << 8));
        self.duty_cycle = 0;
        self.envelope.restart();
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.length.set_enabled(enabled);
    }

    pub const fn volume(&self) -> u8 {
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
        if self.is_muted() {
            0.0
        } else {
            f32::from(
                Self::DUTY_TABLE[self.duty as usize][self.duty_cycle as usize] * self.volume(),
            )
        }
    }
}

impl TimerCycle for Pulse {
    fn cycle(&self) -> usize {
        self.timer.cycle
    }
}

impl Clock for Pulse {
    //                  Sweep -----> Timer
    //                    |            |
    //                    |            |
    //                    |            v
    //                    |        Sequencer   Length Counter
    //                    |            |             |
    //                    |            |             |
    //                    v            v             v
    // Envelope -------> Gate -----> Gate -------> Gate --->(to mixer)
    fn clock(&mut self) -> usize {
        if self.timer.clock() > 0 {
            self.duty_cycle = self.duty_cycle.wrapping_sub(1) & 0x07;
            1
        } else {
            0
        }
    }
}

impl Reset for Pulse {
    fn reset(&mut self, kind: ResetKind) {
        self.timer.reset(kind);
        self.length.reset(kind);
        self.envelope.reset(kind);
        self.sweep.reset(kind);
        self.update_target_period();
        self.duty = 0;
        self.duty_cycle = 0;
    }
}

/// APU Sweep provides frequency sweeping for the APU pulse channels.
///
/// See: <https://www.nesdev.org/wiki/APU_Sweep>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sweep {
    pub enabled: bool,
    pub channel: PulseChannel,
    pub negate: bool, // Treats PulseChannel 1 differently than PulseChannel 2
    pub reload: bool,
    pub shift: u8,
    pub timer: u16,
    pub divider: u8,
    pub period: u8,
    pub target_period: usize,
}

impl Sweep {
    pub const fn new(channel: PulseChannel) -> Self {
        Self {
            enabled: false,
            channel,
            negate: false,
            reload: false,
            shift: 0,
            timer: 0,
            divider: 0,
            period: 0,
            target_period: 0,
        }
    }
}

impl Reset for Sweep {
    fn reset(&mut self, _kind: ResetKind) {
        self.enabled = false;
        self.period = 0;
        self.negate = false;
        self.reload = false;
        self.shift = 0;
        self.divider = 0;
        self.target_period = 0;
    }
}
