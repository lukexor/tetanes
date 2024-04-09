use crate::{
    apu::{envelope::Envelope, length_counter::LengthCounter, timer::Timer, Apu, Channel},
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sample},
};
use serde::{Deserialize, Serialize};

/// Noise shift mode.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum ShiftMode {
    /// Zero (XOR bits 0 and 1)
    Zero,
    /// One (XOR bits 0 and 6)
    One,
}

/// APU Noise Channel provides pseudo-random noise generation.
///
/// See: <https://www.nesdev.org/wiki/APU_Noise>
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Noise {
    pub region: NesRegion,
    pub timer: Timer,
    pub shift: u16,
    pub shift_mode: ShiftMode,
    pub length: LengthCounter,
    pub envelope: Envelope,
    pub force_silent: bool,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new(NesRegion::default())
    }
}

impl Noise {
    const PERIOD_TABLE_NTSC: [usize; 16] = [
        4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
    ];
    const PERIOD_TABLE_PAL: [usize; 16] = [
        4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778,
    ];

    pub const fn new(region: NesRegion) -> Self {
        Self {
            region,
            // Noise channel is clocked at APU rate (CPU / 2)
            timer: Timer::new(Self::period(region, 0), 2),
            shift: 1, // defaults to 1 on power up
            shift_mode: ShiftMode::Zero,
            length: LengthCounter::new(Channel::Noise),
            envelope: Envelope::new(),
            force_silent: false,
        }
    }

    #[must_use]
    pub const fn is_muted(&self) -> bool {
        (self.shift & 0x01) == 0x01 || self.silent()
    }

    #[must_use]
    pub const fn silent(&self) -> bool {
        self.force_silent
    }

    pub fn set_silent(&mut self, silent: bool) {
        self.force_silent = silent;
    }

    const fn period(region: NesRegion, val: u8) -> usize {
        let index = (val & 0x0F) as usize;
        match region {
            NesRegion::Ntsc | NesRegion::Dendy => Self::PERIOD_TABLE_NTSC[index] - 1,
            NesRegion::Pal => Self::PERIOD_TABLE_PAL[index] - 1,
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.clock_quarter_frame();
        self.length.clock();
    }

    /// $400C Noise control
    pub fn write_ctrl(&mut self, val: u8) {
        self.length.write_ctrl((val & 0x20) == 0x20); // !D5
        self.envelope.write_ctrl(val);
    }

    /// $400E Noise timer
    pub fn write_timer(&mut self, val: u8) {
        self.timer.period = Self::period(self.region, val);
        self.shift_mode = if (val & 0x80) == 0x80 {
            ShiftMode::One
        } else {
            ShiftMode::Zero
        };
    }

    /// $400F Length counter
    pub fn write_length(&mut self, val: u8) {
        self.length.write(val >> 3);
        self.envelope.restart();
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.length.set_enabled(enabled);
    }

    pub fn volume(&self) -> u8 {
        if self.length.counter > 0 {
            self.envelope.volume()
        } else {
            0
        }
    }

    pub fn clock_to_output(&mut self, cycle: usize, output: &mut [f32]) -> usize {
        let offset = Channel::Noise as usize;
        let start = self.timer.cycle;
        while self.timer.cycle < cycle {
            //    Timer --> Shift Register   Length Counter
            //                    |                |
            //                    v                v
            // Envelope -------> Gate ----------> Gate --> (to mixer)
            if self.timer.clock() > 0 {
                self.clock();
            }
            output[((self.timer.cycle - 1) * Apu::MAX_CHANNEL_COUNT) + offset] = self.output();
        }
        self.timer.cycle - start
    }
}

impl Sample for Noise {
    #[must_use]
    fn output(&self) -> f32 {
        if self.is_muted() {
            0f32
        } else {
            f32::from(self.volume())
        }
    }
}

impl Clock for Noise {
    fn clock(&mut self) -> usize {
        let shift_by = if self.shift_mode == ShiftMode::One {
            6
        } else {
            1
        };
        let feedback = (self.shift & 0x01) ^ ((self.shift >> shift_by) & 0x01);
        self.shift >>= 1;
        self.shift |= feedback << 14;
        1
    }
}

impl Regional for Noise {
    fn region(&self) -> NesRegion {
        self.region
    }

    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
    }
}

impl Reset for Noise {
    fn reset(&mut self, kind: ResetKind) {
        self.envelope.reset(kind);
        self.length.reset(kind);
        self.timer.period = Self::period(self.region, 0);
        self.shift = 1;
        self.shift_mode = ShiftMode::Zero;
    }
}
