use super::{length_counter::LengthCounter, linear_counter::LinearCounter};
use crate::{
    common::{Clocked, Powered},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

#[derive(Clone)]
pub struct Triangle {
    pub enabled: bool,
    ultrasonic: bool,
    step: u8,
    freq_timer: u16,
    freq_counter: u16,
    pub length: LengthCounter,
    linear: LinearCounter,
}

impl Triangle {
    pub fn new() -> Self {
        Self {
            enabled: false,
            ultrasonic: false,
            step: 0u8,
            freq_timer: 0u16,
            freq_counter: 0u16,
            length: LengthCounter::new(),
            linear: LinearCounter::new(),
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        if self.linear.reload {
            self.linear.counter = self.linear.load;
        } else if self.linear.counter > 0 {
            self.linear.counter -= 1;
        }
        if !self.linear.control {
            self.linear.reload = false;
        }
    }

    pub fn clock_half_frame(&mut self) {
        self.length.clock();
    }

    pub fn output(&self) -> f32 {
        if self.ultrasonic {
            7.5
        } else if self.step & 0x10 == 0x10 {
            f32::from(self.step ^ 0x1F)
        } else {
            f32::from(self.step)
        }
    }

    pub fn write_linear_counter(&mut self, val: u8) {
        self.linear.control = (val >> 7) & 1 == 1; // D7
        self.length.enabled = (val >> 7) & 1 == 0; // !D7
        self.linear.load_value(val);
    }

    pub fn write_timer_lo(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0xFF00) | u16::from(val); // D7..D0
    }

    pub fn write_timer_hi(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0x00FF) | u16::from(val & 0x07) << 8; // D2..D0
        self.freq_counter = self.freq_timer;
        self.linear.reload = true;
        if self.enabled {
            self.length.load_value(val);
        }
    }
}

impl Clocked for Triangle {
    fn clock(&mut self) -> usize {
        self.ultrasonic = false;
        if self.length.counter > 0 && self.freq_timer < 2 && self.freq_counter == 0 {
            self.ultrasonic = true;
        }

        let should_clock =
            !(self.length.counter == 0 || self.linear.counter == 0 || self.ultrasonic);
        if should_clock {
            if self.freq_counter > 0 {
                self.freq_counter -= 1;
            } else {
                self.freq_counter = self.freq_timer;
                self.step = (self.step + 1) & 0x1F;
            }
            1
        } else {
            0
        }
    }
}

impl Powered for Triangle {
    fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Savable for Triangle {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.ultrasonic.save(fh)?;
        self.step.save(fh)?;
        self.freq_timer.save(fh)?;
        self.freq_counter.save(fh)?;
        self.length.save(fh)?;
        self.linear.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.ultrasonic.load(fh)?;
        self.step.load(fh)?;
        self.freq_timer.load(fh)?;
        self.freq_counter.load(fh)?;
        self.length.load(fh)?;
        self.linear.load(fh)?;
        Ok(())
    }
}

impl Default for Triangle {
    fn default() -> Self {
        Self::new()
    }
}
