use crate::common::{Clock, Kind, NesRegion, Regional, Reset};
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Dmc {
    region: NesRegion,
    force_silent: bool,
    irq_enabled: bool,
    irq_pending: bool,
    loops: bool,
    freq_timer: u16,
    freq_counter: u16,
    addr: u16,
    addr_load: u16,
    length: u16,
    length_load: u16,
    sample_buffer: u8,
    sample_buffer_empty: bool,
    dma_pending: bool,
    init: u8,
    output: u8,
    output_bits: u8,
    output_shift: u8,
    output_silent: bool,
}

impl Default for Dmc {
    fn default() -> Self {
        Self::new()
    }
}

impl Dmc {
    const FREQ_TABLE_NTSC: [u16; 16] = [
        0x1AC, 0x17C, 0x154, 0x140, 0x11E, 0x0FE, 0x0E2, 0x0D6, 0x0BE, 0x0A0, 0x08E, 0x080, 0x06A,
        0x054, 0x048, 0x036,
    ];
    const FREQ_TABLE_PAL: [u16; 16] = [
        0x18E, 0x162, 0x13C, 0x12A, 0x114, 0x0EC, 0x0D2, 0x0C6, 0x0B0, 0x094, 0x084, 0x076, 0x062,
        0x04E, 0x042, 0x032,
    ];

    pub fn new() -> Self {
        let region = NesRegion::default();
        let freq_timer = Self::freq_timer(region, 0);
        Self {
            region,
            force_silent: false,
            irq_enabled: false,
            irq_pending: false,
            loops: false,
            freq_timer,
            freq_counter: freq_timer,
            addr: 0xC000,
            addr_load: 0x0000,
            length: 0x0000,
            length_load: 0x0001,
            sample_buffer: 0x00,
            sample_buffer_empty: true,
            dma_pending: false,
            init: 0,
            output: 0x00,
            output_bits: 0x00,
            output_shift: 0x00,
            output_silent: true,
        }
    }

    #[inline]
    #[must_use]
    pub const fn silent(&self) -> bool {
        self.force_silent
    }

    #[inline]
    pub fn toggle_silent(&mut self) {
        self.force_silent = !self.force_silent;
    }

    #[inline]
    #[must_use]
    pub const fn length(&self) -> u16 {
        self.length
    }

    #[inline]
    #[must_use]
    pub const fn irq_enabled(&self) -> bool {
        self.irq_enabled
    }

    #[inline]
    #[must_use]
    pub const fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    #[inline]
    pub fn acknowledge_irq(&mut self) {
        self.irq_pending = false;
    }

    #[inline]
    #[must_use]
    pub fn dma(&mut self) -> bool {
        let pending = self.dma_pending;
        self.dma_pending = false;
        pending
    }

    #[inline]
    #[must_use]
    pub const fn dma_addr(&self) -> u16 {
        self.addr
    }

    #[inline]
    pub fn load_buffer(&mut self, val: u8) {
        self.dma_pending = false;
        if self.length > 0 {
            self.sample_buffer = val;
            self.sample_buffer_empty = false;
            self.addr = self.addr.wrapping_add(1);
            if self.addr == 0 {
                self.addr = 0x8000;
            }
            self.length -= 1;
            if self.length == 0 {
                if self.loops {
                    self.length = self.length_load;
                    self.addr = self.addr_load;
                } else if self.irq_enabled {
                    self.irq_pending = true;
                }
            }
        }
    }

    #[inline]
    const fn freq_timer(region: NesRegion, val: u8) -> u16 {
        match region {
            NesRegion::Ntsc => Self::FREQ_TABLE_NTSC[(val & 0x0F) as usize] - 2,
            NesRegion::Pal | NesRegion::Dendy => Self::FREQ_TABLE_PAL[(val & 0x0F) as usize] - 2,
        }
    }

    #[inline]
    #[must_use]
    pub fn output(&self) -> f32 {
        if self.force_silent {
            0.0
        } else {
            f32::from(self.output)
        }
    }

    // $4010 DMC timer
    pub fn write_timer(&mut self, val: u8) {
        self.irq_enabled = val & 0x80 == 0x80;
        self.loops = val & 0x40 == 0x40;
        self.freq_timer = Self::freq_timer(self.region, val);
        if !self.irq_enabled {
            self.irq_pending = false;
        }
    }

    // $4011 DMC output
    #[inline]
    pub fn write_output(&mut self, val: u8) {
        self.output = val;
    }

    // $4012 DMC addr load
    #[inline]
    pub fn write_addr_load(&mut self, val: u8) {
        self.addr_load = 0xC000 | (u16::from(val) << 6);
    }

    // $4013 DMC length
    #[inline]
    pub fn write_length(&mut self, val: u8) {
        self.length_load = (u16::from(val) << 4) + 1;
    }

    // $4015 WRITE
    pub fn set_enabled(&mut self, enabled: bool, cycle: usize) {
        self.irq_pending = false;
        if !enabled {
            self.length = 0;
        } else if self.length == 0 {
            self.addr = self.addr_load;
            self.length = self.length_load;
            // Delay a number of cycles based on even/odd cycle
            self.init = if cycle & 0x01 == 0x00 { 2 } else { 3 };
        }
    }

    #[inline]
    pub fn check_pending_dma(&mut self) {
        if self.init > 0 {
            self.init -= 1;
            if self.init == 0 && self.sample_buffer_empty && self.length > 0 {
                self.dma_pending = true;
            }
        }
    }
}

impl Clock for Dmc {
    fn clock(&mut self) -> usize {
        // Because APU is only clocked every other CPU cycle
        if self.freq_counter >= 2 {
            self.freq_counter -= 2;
        } else {
            self.freq_counter = self.freq_timer;

            if !self.output_silent {
                if self.output_shift & 0x01 == 0x01 {
                    if self.output <= 125 {
                        self.output += 2;
                    }
                } else if self.output >= 2 {
                    self.output -= 2;
                }
                self.output_shift >>= 1;
            }

            self.output_bits = self.output_bits.saturating_sub(1);
            if self.output_bits == 0 {
                self.output_bits = 8;
                if self.sample_buffer_empty {
                    self.output_silent = true;
                } else {
                    self.output_silent = false;
                    self.output_shift = self.sample_buffer;
                    self.sample_buffer_empty = true;
                    if self.length > 0 {
                        self.dma_pending = true;
                    }
                }
            }
        }
        1
    }
}

impl Regional for Dmc {
    #[inline]
    fn region(&self) -> NesRegion {
        self.region
    }

    #[inline]
    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.freq_timer = Self::freq_timer(region, 0);
    }
}

impl Reset for Dmc {
    fn reset(&mut self, kind: Kind) {
        self.irq_enabled = false;
        self.irq_pending = false;
        self.loops = false;
        self.freq_timer = Self::freq_timer(self.region, 0);
        self.freq_counter = self.freq_timer;
        match kind {
            Kind::Soft => {
                self.addr = 0x0000;
                self.length_load = 0x0000;
            }
            Kind::Hard => {
                self.addr = 0xC000;
                self.length_load = 0x0001;
            }
        }
        self.addr_load = 0x0000;
        self.length = 0x0000;
        self.sample_buffer = 0x00;
        self.sample_buffer_empty = true;
        self.dma_pending = false;
        self.init = 0;
        self.output = 0x00;
        self.output_bits = 0x00;
        self.output_shift = 0x00;
        self.output_silent = true;
    }
}
