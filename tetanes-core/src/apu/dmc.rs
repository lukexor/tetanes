use crate::common::{Clock, NesRegion, Regional, Reset, ResetKind, Sample};
use serde::{Deserialize, Serialize};

/// APU Delta Modulation Channel (DMC) provides sample playback.
///
/// See: <https://www.nesdev.org/wiki/APU_DMC>
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Dmc {
    pub region: NesRegion,
    pub force_silent: bool,
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub loops: bool,
    pub period: u16,
    pub timer: u16,
    pub addr: u16,
    pub sample_addr: u16,
    pub bytes_remaining: u16,
    pub sample_length: u16,
    pub sample_buffer: u8,
    pub buffer_empty: bool,
    pub dma_pending: bool,
    pub init: u8,
    pub output_level: u8,
    pub bits_remaining: u8,
    pub shift: u8,
    pub silence: bool,
}

impl Default for Dmc {
    fn default() -> Self {
        Self::new()
    }
}

impl Dmc {
    const PERIOD_TABLE_NTSC: [u16; 16] = [
        428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
    ];
    const FREQ_TABLE_PAL: [u16; 16] = [
        398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
    ];

    pub fn new() -> Self {
        let region = NesRegion::default();
        let period = Self::period(region, 0);
        Self {
            region,
            force_silent: false,
            irq_enabled: false,
            irq_pending: false,
            loops: false,
            period,
            timer: period,
            addr: 0xC000,
            sample_addr: 0x0000,
            bytes_remaining: 0x0000,
            sample_length: 0x0001,
            sample_buffer: 0x00,
            buffer_empty: true,
            dma_pending: false,
            init: 0,
            output_level: 0x00,
            bits_remaining: 0x00,
            shift: 0x00,
            silence: true,
        }
    }

    #[must_use]
    pub const fn silent(&self) -> bool {
        self.force_silent
    }

    pub fn set_silent(&mut self, silent: bool) {
        self.force_silent = silent;
    }

    #[must_use]
    pub const fn length(&self) -> u16 {
        self.bytes_remaining
    }

    #[must_use]
    pub const fn irq_enabled(&self) -> bool {
        self.irq_enabled
    }

    #[must_use]
    pub const fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    pub fn acknowledge_irq(&mut self) {
        self.irq_pending = false;
    }

    #[must_use]
    pub fn dma(&mut self) -> bool {
        let pending = self.dma_pending;
        self.dma_pending = false;
        pending
    }

    #[must_use]
    pub const fn dma_addr(&self) -> u16 {
        self.addr
    }

    fn init_sample(&mut self) {
        self.addr = self.sample_addr;
        self.bytes_remaining = self.sample_length;
        // TODO: set apu needs to run if self.bytes_remaining > 0
    }

    pub fn load_buffer(&mut self, val: u8) {
        self.dma_pending = false;
        if self.bytes_remaining > 0 {
            self.sample_buffer = val;
            self.buffer_empty = false;
            self.addr = self.addr.wrapping_add(1);
            if self.addr == 0 {
                self.addr = 0x8000;
            }
            self.bytes_remaining -= 1;
            if self.bytes_remaining == 0 {
                // TODO: clear apu needs to run
                if self.loops {
                    self.init_sample();
                } else if self.irq_enabled {
                    self.irq_pending = true;
                }
            }
        }
    }

    const fn period(region: NesRegion, val: u8) -> u16 {
        match region {
            NesRegion::Ntsc | NesRegion::Dendy => {
                Self::PERIOD_TABLE_NTSC[(val & 0x0F) as usize] - 2
            }
            NesRegion::Pal => Self::FREQ_TABLE_PAL[(val & 0x0F) as usize] - 2,
        }
    }

    /// $4010 DMC timer
    pub fn write_timer(&mut self, val: u8) {
        self.irq_enabled = val & 0x80 == 0x80;
        self.loops = val & 0x40 == 0x40;
        self.period = Self::period(self.region, val);
        if !self.irq_enabled {
            self.irq_pending = false;
        }
    }

    /// $4011 DMC output
    pub fn write_output(&mut self, val: u8) {
        self.output_level = val & 0x7F;
    }

    /// $4012 DMC addr load
    pub fn write_addr_load(&mut self, val: u8) {
        self.sample_addr = 0xC000 | (u16::from(val) << 6);
    }

    /// $4013 DMC length
    pub fn write_length(&mut self, val: u8) {
        self.sample_length = (u16::from(val) << 4) | 0x01;
    }

    /// $4015 WRITE
    pub fn set_enabled(&mut self, enabled: bool, cycle: usize) {
        self.irq_pending = false;
        if !enabled {
            self.bytes_remaining = 0;
        } else if self.bytes_remaining == 0 {
            self.init_sample();
            // Delay a number of cycles based on even/odd cycle
            self.init = if cycle & 0x01 == 0x00 { 2 } else { 3 };
        }
    }

    pub fn check_pending_dma(&mut self) {
        if self.init > 0 {
            self.init -= 1;
            if self.init == 0 && self.buffer_empty && self.bytes_remaining > 0 {
                self.dma_pending = true;
            }
        }
    }
}

impl Sample for Dmc {
    #[must_use]
    fn output(&self) -> f32 {
        if self.silent() {
            0.0
        } else {
            f32::from(self.output_level)
        }
    }
}

impl Clock for Dmc {
    fn clock(&mut self) -> usize {
        if self.timer > 1 {
            self.timer -= 2;
            0
        } else {
            self.timer = self.period;

            if !self.silence {
                if self.shift & 0x01 == 0x01 {
                    if self.output_level <= 125 {
                        self.output_level += 2;
                    }
                } else if self.output_level >= 2 {
                    self.output_level -= 2;
                }
                self.shift >>= 1;
            }

            self.bits_remaining = self.bits_remaining.saturating_sub(1);
            if self.bits_remaining == 0 {
                self.bits_remaining = 8;
                if self.buffer_empty {
                    self.silence = true;
                } else {
                    self.silence = false;
                    self.shift = self.sample_buffer;
                    self.buffer_empty = true;
                    if self.bytes_remaining > 0 {
                        self.dma_pending = true;
                    }
                }
            }
            1
        }
    }
}

impl Regional for Dmc {
    fn region(&self) -> NesRegion {
        self.region
    }

    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.period = Self::period(region, 0);
    }
}

impl Reset for Dmc {
    fn reset(&mut self, kind: ResetKind) {
        if let ResetKind::Hard = kind {
            self.addr = 0xC000;
            self.sample_length = 0x0001;
        }
        self.irq_enabled = false;
        self.irq_pending = false;
        self.loops = false;
        self.period = Self::period(self.region, 0);
        self.timer = self.period;
        self.sample_addr = 0x0000;
        self.bytes_remaining = 0x0000;
        self.sample_buffer = 0x00;
        self.buffer_empty = true;
        self.dma_pending = false;
        self.init = 0;
        self.output_level = 0x00;
        self.bits_remaining = 0x00;
        self.shift = 0x00;
        self.silence = true;
    }
}
