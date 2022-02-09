use crate::{
    common::{Clocked, Powered},
    mapper::MapperType,
    memory::MemRead,
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

#[derive(Debug, Copy, Clone)]
#[must_use]
pub struct Dmc {
    pub(super) mapper: *mut MapperType,
    pub irq_enabled: bool,
    pub irq_pending: bool,
    loops: bool,
    freq_timer: u16,
    freq_counter: u16,
    pub addr: u16,
    pub addr_load: u16,
    pub length: u8,
    pub length_load: u8,
    sample_buffer: u8,
    sample_buffer_empty: bool,
    pub output: u8,
    output_bits: u8,
    output_shift: u8,
    output_silent: bool,
}

impl Dmc {
    // NTSC
    const NTSC_FREQ_TABLE: [u16; 16] = [
        0x1AC, 0x17C, 0x154, 0x140, 0x11E, 0x0FE, 0x0E2, 0x0D6, 0x0BE, 0x0A0, 0x08E, 0x080, 0x06A,
        0x054, 0x048, 0x036,
    ];

    pub const fn new() -> Self {
        Self {
            mapper: std::ptr::null_mut(),
            irq_enabled: false,
            irq_pending: false,
            loops: false,
            freq_timer: 0u16,
            freq_counter: 0u16,
            addr: 0u16,
            addr_load: 0u16,
            length: 0u8,
            length_load: 0u8,
            sample_buffer: 0u8,
            sample_buffer_empty: false,
            output: 0u8,
            output_bits: 0u8,
            output_shift: 0u8,
            output_silent: false,
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
        self.irq_enabled = (val >> 7) & 1 == 1;
        self.loops = (val >> 6) & 1 == 1;
        self.freq_timer = Self::NTSC_FREQ_TABLE[(val & 0x0F) as usize];
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
        self.length_load = (val << 4) + 1;
    }

    #[inline]
    fn mapper_mut(&mut self) -> &mut MapperType {
        unsafe { &mut *self.mapper }
    }
}

impl Clocked for Dmc {
    #[inline]
    fn clock(&mut self) -> usize {
        if self.freq_counter > 0 {
            self.freq_counter -= 1;
        } else {
            self.freq_counter = self.freq_timer;
            if !self.output_silent {
                if self.output_shift & 1 == 1 {
                    if self.output <= 0x7D {
                        self.output += 2;
                    }
                } else if self.output >= 0x02 {
                    self.output -= 2;
                }
            }
            self.output_shift >>= 1;

            self.output_bits = self.output_bits.saturating_sub(1);
            if self.output_bits == 0 {
                self.output_bits = 8;
                if self.sample_buffer_empty {
                    self.output_silent = true;
                } else {
                    self.output_shift = self.sample_buffer;
                    self.sample_buffer_empty = true;
                    self.output_silent = false;
                }
            }
        }

        if self.length > 0 && self.sample_buffer_empty {
            let addr = self.addr;
            self.sample_buffer = self.mapper_mut().read(addr);
            self.sample_buffer_empty = false;
            self.addr = self.addr.wrapping_add(1) | 0x8000;
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
        1
    }
}

impl Powered for Dmc {
    fn reset(&mut self) {
        self.irq_enabled = false;
        self.irq_pending = false;
        self.loops = false;
        self.freq_timer = 0;
        self.freq_counter = 0;
        self.addr = 0;
        self.addr_load = 0;
        self.length = 0;
        self.length_load = 0;
        self.sample_buffer = 0;
        self.sample_buffer_empty = false;
        self.output = 0;
        self.output_bits = 0;
        self.output_shift = 0;
        self.output_silent = false;
    }
}

impl Savable for Dmc {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        // Ignore mapper
        self.irq_enabled.save(fh)?;
        self.irq_pending.save(fh)?;
        self.loops.save(fh)?;
        self.freq_timer.save(fh)?;
        self.freq_counter.save(fh)?;
        self.addr.save(fh)?;
        self.addr_load.save(fh)?;
        self.length.save(fh)?;
        self.length_load.save(fh)?;
        self.sample_buffer.save(fh)?;
        self.sample_buffer_empty.save(fh)?;
        self.output.save(fh)?;
        self.output_bits.save(fh)?;
        self.output_shift.save(fh)?;
        self.output_silent.save(fh)?;
        // Ignore log_level
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.irq_enabled.load(fh)?;
        self.irq_pending.load(fh)?;
        self.loops.load(fh)?;
        self.freq_timer.load(fh)?;
        self.freq_counter.load(fh)?;
        self.addr.load(fh)?;
        self.addr_load.load(fh)?;
        self.length.load(fh)?;
        self.length_load.load(fh)?;
        self.sample_buffer.load(fh)?;
        self.sample_buffer_empty.load(fh)?;
        self.output.load(fh)?;
        self.output_bits.load(fh)?;
        self.output_shift.load(fh)?;
        self.output_silent.load(fh)?;
        Ok(())
    }
}

impl Default for Dmc {
    fn default() -> Self {
        Self::new()
    }
}
