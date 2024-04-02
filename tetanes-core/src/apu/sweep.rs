use super::pulse::PulseChannel;
use crate::common::{Clock, Reset, ResetKind};
use serde::{Deserialize, Serialize};

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
    pub divider_period: u8,
    pub period: u16,
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
            divider_period: 0,
            period: 0,
            target_period: 0,
        }
    }

    #[inline]
    pub fn is_muted(&self) -> bool {
        self.period < 8 || (!self.negate && self.target_period > 0x7FF)
    }

    #[inline]
    pub fn write(&mut self, val: u8) {
        self.enabled = (val & 0x80) == 0x80;
        self.divider_period = ((val & 0x70) >> 4) + 1;
        self.negate = (val & 0x08) == 0x08;
        self.shift = val & 0x07;
        self.update_target_period();
        self.reload = true;
    }

    pub fn write_timer_lo(&mut self, val: u8) {
        self.period = (self.period & 0x0700) | u16::from(val);
        self.update_target_period();
    }

    pub fn write_timer_hi(&mut self, val: u8) {
        self.period = (self.period & 0xFF) | (u16::from(val & 0x07) << 8);
        self.update_target_period();
    }

    fn update_target_period(&mut self) {
        let delta = self.period >> self.shift;
        if self.negate {
            self.target_period = self.period - delta;
            if let PulseChannel::One = self.channel {
                self.target_period = self.target_period.wrapping_sub(1);
            }
        } else {
            self.target_period = self.period + delta;
        }
    }
}

impl Clock for Sweep {
    fn clock(&mut self) -> usize {
        self.divider = self.divider.wrapping_sub(1);
        let mut clock = 0;
        if self.divider == 0 {
            if self.shift > 0 && self.enabled && self.period >= 8 && self.target_period <= 0x7FF {
                self.period = self.target_period;
                clock = 1
            }
            self.divider = self.divider_period;
        }

        if self.reload {
            self.divider = self.divider_period;
            self.reload = false;
        }

        clock
    }
}

impl Reset for Sweep {
    fn reset(&mut self, _kind: ResetKind) {
        self.enabled = false;
        self.divider_period = 0;
        self.negate = false;
        self.reload = false;
        self.shift = 0;
        self.divider = 0;
        self.target_period = 0;
        self.update_target_period();
    }
}
