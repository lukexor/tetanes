use crate::{
    apu::length_counter::LengthCounter,
    common::{Clock, Reset, ResetKind, Sample},
};
use serde::{Deserialize, Serialize};

/// APU Triangle Channel provides triangle wave generation.
///
/// See: <https://www.nesdev.org/wiki/APU_Triangle>
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Triangle {
    pub timer: u16,
    pub period: u16,
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
            timer: 0,
            period: 0,
            sequence: 0,
            length: LengthCounter::new(),
            linear: LinearCounter::new(),
            force_silent: false,
        }
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
        self.linear.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.length.clock();
    }

    /// $4008 Linear counter control
    pub fn write_linear_counter(&mut self, val: u8) {
        self.linear.control = (val & 0x80) == 0x80; // D7
        self.linear.write(val & 0x7F); // D6..D0;
        self.length.write_ctrl(self.linear.control); // !D7
    }

    /// $400A Triangle timer lo
    pub fn write_timer_lo(&mut self, val: u8) {
        self.period = (self.period & 0xFF00) | u16::from(val); // D7..D0
    }

    /// $400B Triangle timer high
    pub fn write_timer_hi(&mut self, val: u8) {
        self.length.write(val >> 3);
        self.period = (self.period & 0x00FF) | u16::from(val & 0x07) << 8; // D2..D0
        self.linear.reload = true;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.length.set_enabled(enabled);
    }
}

impl Sample for Triangle {
    #[must_use]
    fn output(&self) -> f32 {
        if !self.silent() {
            f32::from(Self::SEQUENCE[self.sequence as usize])
        } else {
            0.0
        }
    }
}

impl Clock for Triangle {
    fn clock(&mut self) -> usize {
        if self.timer == 0 && self.length.counter > 0 && self.linear.counter > 0 {
            self.sequence = (self.sequence + 1) & 0x1F;
            self.timer = self.period;
            1
        } else {
            if self.timer > 0 {
                self.timer -= 1;
            }
            0
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

    pub fn write(&mut self, val: u8) {
        self.counter_reload = val;
    }
}

impl Clock for LinearCounter {
    fn clock(&mut self) -> usize {
        if self.reload {
            self.counter = self.counter_reload;
        } else if self.counter > 0 {
            self.counter -= 1;
        }
        if !self.control {
            self.reload = false;
        }
        1
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
