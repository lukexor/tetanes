use super::{envelope::Envelope, length_counter::LengthCounter};
use crate::{
    common::{Clocked, Powered},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

#[derive(PartialEq, Eq, Copy, Clone)]
enum ShiftMode {
    Zero,
    One,
}

#[derive(Clone)]
pub struct Noise {
    pub enabled: bool,
    freq_timer: u16,       // timer freq_counter reload value
    freq_counter: u16,     // Current frequency timer value
    shift: u16,            // Must never be 0
    shift_mode: ShiftMode, // Zero (XOR bits 0 and 1) or One (XOR bits 0 and 6)
    pub length: LengthCounter,
    envelope: Envelope,
}

impl Noise {
    const FREQ_TABLE: [u16; 16] = [
        4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
    ];
    const SHIFT_BIT_15_MASK: u16 = !0x8000;

    pub fn new() -> Self {
        Self {
            enabled: false,
            freq_timer: 0u16,
            freq_counter: 0u16,
            shift: 1u16, // Must never be 0
            shift_mode: ShiftMode::Zero,
            length: LengthCounter::new(),
            envelope: Envelope::new(),
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.length.clock();
    }

    pub fn output(&self) -> f32 {
        if self.shift & 1 == 0 && self.length.counter != 0 {
            if self.envelope.enabled {
                f32::from(self.envelope.volume)
            } else {
                f32::from(self.envelope.constant_volume)
            }
        } else {
            0f32
        }
    }

    pub fn write_control(&mut self, val: u8) {
        self.length.write_control(val);
        self.envelope.write_control(val);
    }

    // $400E Noise timer
    pub fn write_timer(&mut self, val: u8) {
        self.freq_timer = Self::FREQ_TABLE[(val & 0x0F) as usize];
        self.shift_mode = if (val >> 7) & 1 == 1 {
            ShiftMode::One
        } else {
            ShiftMode::Zero
        };
    }

    pub fn write_length(&mut self, val: u8) {
        if self.enabled {
            self.length.load_value(val);
        }
        self.envelope.reset = true;
    }
}

impl Clocked for Noise {
    fn clock(&mut self) -> usize {
        if self.freq_counter > 0 {
            self.freq_counter -= 1;
        } else {
            self.freq_counter = self.freq_timer;
            let shift_amount = if self.shift_mode == ShiftMode::One {
                6
            } else {
                1
            };
            let bit1 = self.shift & 1; // Bit 0
            let bit2 = (self.shift >> shift_amount) & 1; // Bit 1 or 6 from above
            self.shift = (self.shift & Self::SHIFT_BIT_15_MASK) | ((bit1 ^ bit2) << 14);
            self.shift >>= 1;
        }
        1
    }
}

impl Powered for Noise {
    fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Savable for Noise {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.freq_timer.save(fh)?;
        self.freq_counter.save(fh)?;
        self.shift.save(fh)?;
        self.shift_mode.save(fh)?;
        self.length.save(fh)?;
        self.envelope.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.freq_timer.load(fh)?;
        self.freq_counter.load(fh)?;
        self.shift.load(fh)?;
        self.shift_mode.load(fh)?;
        self.length.load(fh)?;
        self.envelope.load(fh)?;
        Ok(())
    }
}

impl Savable for ShiftMode {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => ShiftMode::Zero,
            1 => ShiftMode::One,
            _ => panic!("invalid ShiftMode value"),
        };
        Ok(())
    }
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}
