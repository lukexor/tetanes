use super::{envelope::Envelope, LengthCounter};
use crate::common::{Clocked, NesFormat, Powered};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
enum ShiftMode {
    Zero,
    One,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Noise {
    nes_format: NesFormat,
    pub enabled: bool,
    freq_timer: u16,       // timer freq_counter reload value
    freq_counter: u16,     // Current frequency timer value
    shift: u16,            // Must never be 0
    shift_mode: ShiftMode, // Zero (XOR bits 0 and 1) or One (XOR bits 0 and 6)
    pub length: LengthCounter,
    envelope: Envelope,
}

impl Noise {
    const FREQ_TABLE_NTSC: [u16; 16] = [
        4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
    ];
    const FREQ_TABLE_PAL: [u16; 16] = [
        4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778,
    ];
    const SHIFT_BIT_15_MASK: u16 = !0x8000;

    pub const fn new(nes_format: NesFormat) -> Self {
        Self {
            nes_format,
            enabled: false,
            freq_timer: 0u16,
            freq_counter: 0u16,
            shift: 1u16, // Must never be 0
            shift_mode: ShiftMode::Zero,
            length: LengthCounter::new(),
            envelope: Envelope::new(),
        }
    }

    #[inline]
    pub fn set_nes_format(&mut self, nes_format: NesFormat) {
        self.nes_format = nes_format;
    }

    #[inline]
    fn freq_timer(nes_format: NesFormat, val: u8) -> u16 {
        match nes_format {
            NesFormat::Ntsc => Self::FREQ_TABLE_NTSC[(val & 0x0F) as usize] - 1,
            NesFormat::Pal | NesFormat::Dendy => Self::FREQ_TABLE_PAL[(val & 0x0F) as usize] - 1,
        }
    }

    #[inline]
    pub fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    #[inline]
    pub fn clock_half_frame(&mut self) {
        self.length.clock();
    }

    #[inline]
    #[must_use]
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

    #[inline]
    pub fn write_control(&mut self, val: u8) {
        self.length.write_control(val);
        self.envelope.write_control(val);
    }

    // $400E Noise timer
    #[inline]
    pub fn write_timer(&mut self, val: u8) {
        self.freq_timer = Self::freq_timer(self.nes_format, val);
        self.shift_mode = if (val >> 7) & 1 == 1 {
            ShiftMode::One
        } else {
            ShiftMode::Zero
        };
    }

    #[inline]
    pub fn write_length(&mut self, val: u8) {
        if self.enabled {
            self.length.load_value(val);
        }
        self.envelope.reset = true;
    }

    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.length.counter = 0;
        }
    }
}

impl Clocked for Noise {
    #[inline]
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
        *self = Self::new(NesFormat::default());
    }
}

impl Default for Noise {
    fn default() -> Self {
        Self::new(NesFormat::default())
    }
}
