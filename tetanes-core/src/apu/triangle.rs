//! APU Triangle Channel implementation.
//!
//! See: <https://www.nesdev.org/wiki/APU_Triangle>

use crate::{
    apu::{
        Channel,
        length_counter::LengthCounter,
        timer::{Timer, TimerCycle},
    },
    common::{Clock, Reset, ResetKind, Sample},
};
use serde::{Deserialize, Serialize};

/// APU Triangle Channel provides triangle wave generation.
///
/// See: <https://www.nesdev.org/wiki/APU_Triangle>
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Triangle {
    pub timer: Timer,
    pub sequence: u8,
    pub length: LengthCounter,
    pub linear: LinearCounter,
    pub force_silent: bool,
}

impl Default for Triangle {
    fn default() -> Self {
        Self::new()
    }
}

impl Triangle {
    const SEQUENCE: [u8; 32] = [
        15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
        12, 13, 14, 15,
    ];

    pub const fn new() -> Self {
        Self {
            timer: Timer::new(0),
            sequence: 0,
            length: LengthCounter::new(Channel::Triangle),
            linear: LinearCounter::new(),
            force_silent: false,
        }
    }

    #[must_use]
    pub const fn silent(&self) -> bool {
        self.force_silent
    }

    pub const fn set_silent(&mut self, silent: bool) {
        self.force_silent = silent;
    }

    pub fn clock_quarter_frame(&mut self) {
        self.linear.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.clock_quarter_frame();
        self.length.clock();
    }

    /// $4008 Linear counter control
    pub const fn write_linear_counter(&mut self, val: u8) {
        self.linear.control = (val & 0x80) == 0x80; // D7
        self.linear.write(val & 0x7F); // D6..D0;
        self.length.write_ctrl(self.linear.control); // !D7
    }

    /// $400A Triangle timer lo
    pub fn write_timer_lo(&mut self, val: u8) {
        self.timer.period = (self.timer.period & 0xFF00) | u16::from(val); // D7..D0
    }

    /// $400B Triangle timer high
    pub fn write_timer_hi(&mut self, val: u8) {
        self.length.write(val >> 3);
        self.timer.period = (self.timer.period & 0x00FF) | (u16::from(val & 0x07) << 8); // D2..D0
        self.linear.reload = true;
    }

    pub const fn set_enabled(&mut self, enabled: bool) {
        self.length.set_enabled(enabled);
    }
}

impl Sample for Triangle {
    fn output(&self) -> f32 {
        if self.silent() {
            0.0
        } else if self.timer.period < 2 {
            // This is normally silenced by a lowpass filter on real hardware
            // See: https://forums.nesdev.org/viewtopic.php?t=10658
            7.5
        } else {
            f32::from(Self::SEQUENCE[self.sequence as usize])
        }
    }
}

impl TimerCycle for Triangle {
    fn cycle(&self) -> u32 {
        self.timer.cycle
    }
}

impl Clock for Triangle {
    //       Linear Counter   Length Counter
    //             |                |
    //             v                v
    // Timer ---> Gate ----------> Gate ---> Sequencer ---> (to mixer)
    fn clock(&mut self) {
        if self.timer.tick() && self.length.counter > 0 && self.linear.counter > 0 {
            self.sequence = (self.sequence + 1) & 0x1F;
        }
    }
}

impl Reset for Triangle {
    fn reset(&mut self, kind: ResetKind) {
        self.length.reset(kind);
        self.linear.reset(kind);
        self.sequence = 0;
    }
}

/// APU Linear Counter provides duration control for the APU triangle channel.
///
/// See: <https://www.nesdev.org/wiki/APU_Triangle>
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct LinearCounter {
    pub reload: bool,
    pub control: bool,
    pub counter_reload: u8,
    pub counter: u8,
}

impl LinearCounter {
    pub const fn new() -> Self {
        Self {
            reload: false,
            control: false,
            counter_reload: 0u8,
            counter: 0u8,
        }
    }

    pub const fn write(&mut self, val: u8) {
        self.counter_reload = val;
    }
}

impl Clock for LinearCounter {
    fn clock(&mut self) {
        if self.reload {
            self.counter = self.counter_reload;
        } else if self.counter > 0 {
            self.counter -= 1;
        }
        if !self.control {
            self.reload = false;
        }
    }
}

impl Reset for LinearCounter {
    fn reset(&mut self, _kind: ResetKind) {
        self.counter = 0;
        self.counter_reload = 0;
        self.reload = false;
        self.control = false;
    }
}
