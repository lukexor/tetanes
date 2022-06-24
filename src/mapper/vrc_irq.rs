//! `VrcIrq`
//!
//! <https://www.nesdev.org/wiki/VRC_IRQ>

use crate::common::{Clocked, Powered};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct VrcIrq {
    reload: u8,
    counter: u8,
    prescalar_counter: i16,
    enabled: bool,
    enabled_after_ack: bool,
    cycle_mode: bool,
    pending: bool,
}

impl VrcIrq {
    #[inline]
    pub fn write_reload(&mut self, val: u8) {
        self.reload = val;
    }

    pub fn write_control(&mut self, val: u8) {
        self.enabled_after_ack = val & 0x01 == 0x01;
        self.enabled = val & 0x02 == 0x02;
        self.cycle_mode = val & 0x04 == 0x04;

        if self.enabled {
            self.counter = self.reload;
            self.prescalar_counter = 341;
        }

        self.pending = false;
    }

    #[inline]
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.pending
    }

    #[inline]
    pub fn acknowledge(&mut self) {
        self.enabled = self.enabled_after_ack;
        self.pending = false;
    }
}

impl Clocked for VrcIrq {
    fn clock(&mut self) -> usize {
        if self.enabled {
            self.prescalar_counter -= 3;
            if self.cycle_mode || self.prescalar_counter <= 0 {
                if self.counter == 0xFF {
                    self.counter = self.reload;
                    self.pending = true;
                } else {
                    self.counter += 1;
                }
                self.prescalar_counter += 341;
            }
            1
        } else {
            0
        }
    }
}

impl Powered for VrcIrq {
    fn reset(&mut self) {
        self.reload = 0;
        self.counter = 0;
        self.prescalar_counter = 0;
        self.enabled = false;
        self.enabled_after_ack = false;
        self.cycle_mode = false;
    }
}
