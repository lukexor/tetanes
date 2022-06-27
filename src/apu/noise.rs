use super::{envelope::Envelope, LengthCounter};
use crate::common::{Clock, Kind, NesRegion, Reset};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
enum ShiftMode {
    Zero,
    One,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Noise {
    region: NesRegion,
    pub enabled: bool,
    freq_timer: u16,       // timer freq_counter reload value
    freq_counter: u16,     // Current frequency timer value
    shift: u16,            // Must never be 0
    shift_mode: ShiftMode, // Zero (XOR bits 0 and 1) or One (XOR bits 0 and 6)
    length: LengthCounter,
    envelope: Envelope,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}

impl Noise {
    const FREQ_TABLE_NTSC: [u16; 16] = [
        4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
    ];
    const FREQ_TABLE_PAL: [u16; 16] = [
        4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778,
    ];
    const SHIFT_BIT_15_MASK: u16 = !0x8000;

    pub fn new() -> Self {
        Self {
            region: NesRegion::default(),
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
    #[must_use]
    pub const fn length_counter(&self) -> u8 {
        self.length.counter()
    }

    #[inline]
    pub fn set_region(&mut self, region: NesRegion) {
        self.region = region;
    }

    #[inline]
    const fn freq_timer(region: NesRegion, val: u8) -> u16 {
        match region {
            NesRegion::Ntsc => Self::FREQ_TABLE_NTSC[(val & 0x0F) as usize] - 1,
            NesRegion::Pal | NesRegion::Dendy => Self::FREQ_TABLE_PAL[(val & 0x0F) as usize] - 1,
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

    pub fn write_ctrl(&mut self, val: u8) {
        self.length.write_ctrl(val);
        self.envelope.write_ctrl(val);
    }

    // $400E Noise timer
    pub fn write_timer(&mut self, val: u8) {
        self.freq_timer = Self::freq_timer(self.region, val);
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

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.length.counter = 0;
        }
    }
}

impl Clock for Noise {
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

impl Reset for Noise {
    fn reset(&mut self, _kind: Kind) {
        *self = Self::new();
    }
}
