use crate::common::{Clocked, NesFormat, Powered};
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Dmc {
    pub nes_format: NesFormat,
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub loops: bool,
    pub freq_timer: u16,
    pub freq_counter: u16,
    pub addr: u16,
    pub addr_load: u16,
    pub length: u16,
    pub length_load: u16,
    pub sample_buffer: u8,
    pub sample_buffer_empty: bool,
    pub dma_pending: bool,
    pub init: u8,
    pub output: u8,
    pub output_bits: u8,
    pub output_shift: u8,
    pub output_silent: bool,
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

    pub fn new(nes_format: NesFormat) -> Self {
        let freq_timer = Self::freq_timer(nes_format, 0);
        Self {
            nes_format,
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
    pub fn set_nes_format(&mut self, nes_format: NesFormat) {
        self.nes_format = nes_format;
        self.freq_timer = Self::freq_timer(nes_format, 0);
    }

    #[inline]
    fn freq_timer(nes_format: NesFormat, val: u8) -> u16 {
        match nes_format {
            NesFormat::Ntsc => Self::FREQ_TABLE_NTSC[(val & 0x0F) as usize] - 2,
            NesFormat::Pal | NesFormat::Dendy => Self::FREQ_TABLE_PAL[(val & 0x0F) as usize] - 2,
        }
    }

    #[must_use]
    #[inline]
    pub fn output(&self) -> f32 {
        f32::from(self.output)
    }

    // $4010 DMC timer
    #[inline]
    pub fn write_timer(&mut self, val: u8) {
        self.irq_enabled = val & 0x80 == 0x80;
        self.loops = val & 0x40 == 0x40;
        self.freq_timer = Self::freq_timer(self.nes_format, val);
        if !self.irq_enabled {
            self.irq_pending = false;
        }
    }

    // $4011 DMC output
    #[inline]
    pub fn write_output(&mut self, val: u8) {
        self.output = val >> 1;
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
    #[inline]
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
    pub fn set_sample_buffer(&mut self, val: u8) {
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
    pub fn check_pending_dma(&mut self) {
        if self.init > 0 {
            self.init -= 1;
            if self.init == 0 && self.sample_buffer_empty && self.length > 0 {
                self.dma_pending = true;
            }
        }
    }
}

impl Clocked for Dmc {
    #[inline]
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

impl Powered for Dmc {
    fn reset(&mut self) {
        self.irq_enabled = false;
        self.irq_pending = false;
        self.loops = false;
        self.freq_timer = Self::freq_timer(self.nes_format, 0);
        self.freq_counter = self.freq_timer;
        self.addr = 0x0000;
        self.addr_load = 0x0000;
        self.length = 0x0000;
        self.length_load = 0x0000;
        self.sample_buffer = 0x00;
        self.sample_buffer_empty = true;
        self.dma_pending = false;
        self.init = 0;
        self.output = 0x00;
        self.output_bits = 0x00;
        self.output_shift = 0x00;
        self.output_silent = true;
    }

    fn power_cycle(&mut self) {
        self.reset();
        self.addr = 0xC000;
        self.length_load = 0x0001;
    }
}

impl Default for Dmc {
    fn default() -> Self {
        Self::new(NesFormat::default())
    }
}
