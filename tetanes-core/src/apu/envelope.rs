use crate::common::{Clock, Reset, ResetKind};
use serde::{Deserialize, Serialize};

/// APU Envelope provides volume control for APU waveform channels.
///
/// See: <https://www.nesdev.org/wiki/APU_Envelope>
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Envelope {
    pub start: bool,
    pub constant_volume: bool,
    pub volume: u8,
    pub divider: u8,
    pub counter: u8,
    pub loops: bool,
}

impl Envelope {
    pub const fn new() -> Self {
        Self {
            start: false,
            constant_volume: false,
            volume: 0,
            divider: 0,
            counter: 0,
            loops: false,
        }
    }

    #[inline]
    #[must_use]
    pub const fn volume(&self) -> u8 {
        if self.constant_volume {
            self.volume
        } else {
            self.counter
        }
    }

    #[inline]
    pub fn restart(&mut self) {
        self.start = true;
    }

    /// $4000/$4004/$400C Envelope control
    #[inline]
    pub fn write_ctrl(&mut self, val: u8) {
        self.loops = (val & 0x20) == 0x20; // D5
        self.constant_volume = (val & 0x10) == 0x10; // D4
        self.volume = val & 0x0F; // D3..D0
    }
}

impl Clock for Envelope {
    fn clock(&mut self) -> usize {
        if self.start {
            self.start = false;
            self.counter = 15;
            self.divider = self.volume;
        } else if self.divider > 0 {
            self.divider -= 1;
        } else {
            self.divider = self.volume;
            if self.counter > 0 {
                self.counter -= 1;
            } else if self.loops {
                self.counter = 15;
            }
        }

        1
    }
}

impl Reset for Envelope {
    fn reset(&mut self, _kind: ResetKind) {
        self.start = false;
        self.constant_volume = false;
        self.volume = 0;
        self.divider = 0;
        self.counter = 0;
    }
}
