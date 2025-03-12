//! `VrcIrq`
//!
//! <https://www.nesdev.org/wiki/VRC_IRQ>

use crate::{
    common::{Clock, Reset, ResetKind},
    cpu::{Cpu, Irq},
};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct VrcIrq {
    pub reload: u8,
    pub counter: u8,
    pub prescalar_counter: i16,
    pub enabled: bool,
    pub enabled_after_ack: bool,
    pub cycle_mode: bool,
}

impl VrcIrq {
    pub const fn write_reload(&mut self, val: u8) {
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

        Cpu::clear_irq(Irq::MAPPER);
    }

    pub fn acknowledge(&mut self) {
        self.enabled = self.enabled_after_ack;
        Cpu::clear_irq(Irq::MAPPER);
    }
}

impl Clock for VrcIrq {
    fn clock(&mut self) -> u64 {
        if self.enabled {
            self.prescalar_counter -= 3;
            if self.cycle_mode || self.prescalar_counter <= 0 {
                if self.counter == 0xFF {
                    self.counter = self.reload;
                    Cpu::set_irq(Irq::MAPPER);
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

impl Reset for VrcIrq {
    fn reset(&mut self, _kind: ResetKind) {
        self.reload = 0;
        self.counter = 0;
        self.prescalar_counter = 0;
        self.enabled = false;
        self.enabled_after_ack = false;
        self.cycle_mode = false;
    }
}
