//! APU Pulse Channel implementation.
//!
//! See: <https://www.nesdev.org/wiki/APU_Pulse>

// The APU's internal wiring: channel state, dividers and filter taps, whose meaning is the
// hardware's rather than this crate's. Public for embedders and debuggers, not a stable surface -
// see the module docs on `apu`.
#![allow(missing_docs)]

use crate::{
    apu::{Channel, envelope::Envelope, length_counter::LengthCounter, timer::Timer},
    common::ResetKind,
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
    pub real_period: u16,
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

    pub const fn set_silent(&mut self, silent: bool) {
        self.force_silent = silent;
    }

    const fn update_target_period(&mut self) {
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

    const fn set_period(&mut self, period: u16) {
        self.real_period = period;
        self.timer.period = (period * 2) + 1;
        self.update_target_period();
    }

    const fn clock_sweep(&mut self) {
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

    pub const fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    pub const fn clock_half_frame(&mut self) {
        self.clock_quarter_frame();
        self.length.clock();
        self.clock_sweep();
    }

    /// $4000/$4004 Pulse control
    pub const fn write_ctrl(&mut self, val: u8) {
        self.length.write_ctrl((val & 0x20) == 0x20); // !D5
        self.envelope.write_ctrl(val);
        self.duty = (val & 0xC0) >> 6;
    }

    /// $4001/$4005 Pulse sweep
    pub const fn write_sweep(&mut self, val: u8) {
        self.sweep.enabled = (val & 0x80) == 0x80;
        self.sweep.negate = (val & 0x08) == 0x08;
        self.sweep.period = ((val & 0x70) >> 4) + 1;
        self.sweep.shift = val & 0x07;
        self.update_target_period();
        self.sweep.reload = true;
    }

    /// $4002/$4006 Pulse timer lo
    pub fn write_timer_lo(&mut self, val: u8) {
        self.set_period(self.real_period & 0x0700 | u16::from(val));
    }

    /// $4003/$4007 Pulse timer hi
    pub fn write_timer_hi(&mut self, val: u8) {
        self.length.write(val >> 3);
        self.set_period(self.real_period & 0xFF | (u16::from(val & 0x07) << 8));
        self.duty_cycle = 0;
        self.envelope.restart();
    }

    pub const fn set_enabled(&mut self, enabled: bool) {
        self.length.set_enabled(enabled);
    }

    pub const fn volume(&self) -> u8 {
        if self.length.counter > 0 {
            self.envelope.volume()
        } else {
            0
        }
    }
    /// Raw channel level, 0-15, before conversion to a float sample.
    ///
    /// Callers that only need to index a mixer lookup table should use this rather than casting
    /// `output()` back to an integer, which costs a saturating float-to-int conversion.
    #[inline]
    pub fn level(&self) -> u8 {
        if self.is_muted() {
            0
        } else {
            Self::DUTY_TABLE[self.duty as usize][self.duty_cycle as usize] * self.volume()
        }
    }
    pub fn output(&self) -> f32 {
        f32::from(self.level())
    }
    pub const fn cycle(&self) -> u32 {
        self.timer.cycle
    }
    //                  Sweep -----> Timer
    //                    |            |
    //                    |            |
    //                    |            v
    //                    |        Sequencer   Length Counter
    //                    |            |             |
    //                    |            |             |
    //                    v            v             v
    // Envelope -------> Gate -----> Gate -------> Gate --->(to mixer)
    pub const fn clock(&mut self) {
        if self.timer.tick() {
            self.duty_cycle = self.duty_cycle.wrapping_sub(1) & 0x07;
        }
    }
    pub fn reset(&mut self, kind: ResetKind) {
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
    pub target_period: u16,
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
    pub const fn reset(&mut self, _kind: ResetKind) {
        self.enabled = false;
        self.period = 0;
        self.negate = false;
        self.reload = false;
        self.shift = 0;
        self.divider = 0;
        self.target_period = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writing `$4003`/`$4007` restarts the duty sequencer but leaves the timer divider running.
    ///
    /// This is what `test_roms/apu/phase_reset.nes` demonstrates audibly; asserting it directly is
    /// both stricter and checkable, since that ROM has no pass/fail output of its own.
    #[test]
    fn writing_timer_hi_resets_the_duty_phase_but_not_the_divider() {
        let mut pulse = Pulse::new(PulseChannel::One, OutputFreq::Default);
        pulse.set_enabled(true);
        pulse.write_timer_lo(0x40);
        pulse.write_timer_hi(0x00);

        // Advance far enough that both the sequencer and the divider are mid-stride.
        for _ in 0..100 {
            pulse.clock();
        }
        let divider = pulse.timer.counter;
        assert_ne!(pulse.duty_cycle, 0, "the sequencer has advanced");
        assert_ne!(divider, pulse.timer.period, "the divider is mid-count");

        pulse.write_timer_hi(0x00);
        assert_eq!(pulse.duty_cycle, 0, "the duty phase restarts");
        assert_eq!(
            pulse.timer.counter, divider,
            "but the divider keeps counting from where it was"
        );
    }
}
