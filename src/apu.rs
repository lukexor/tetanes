//! Audio Processing Unit
//!
//! [https://wiki.nesdev.com/w/index.php/APU]()

use crate::{
    common::{Clocked, Powered},
    cpu::CPU_CLOCK_RATE,
    filter::{Filter, FilterType, HiPassFilter, LoPassFilter},
    mapper::MapperType,
    memory::{MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use dmc::Dmc;
use frame_sequencer::{FcMode, FrameSequencer};
use noise::Noise;
use pulse::{Pulse, PulseChannel};
use std::{
    fmt,
    io::{Read, Write},
};
use triangle::Triangle;

pub const SAMPLE_RATE: f32 = 48_000.0; // in Hz
const SAMPLE_BUFFER_SIZE: usize = 4096;

pub mod dmc;
pub mod noise;
pub mod pulse;
pub mod triangle;

mod divider;
mod envelope;
mod frame_sequencer;
mod length_counter;
mod linear_counter;
mod sequencer;
mod sweep;

/// Audio Processing Unit
#[derive(Clone)]
pub struct Apu {
    pub irq_pending: bool, // Set by $4017 if irq_enabled is clear or set during step 4 of Step4 mode
    irq_enabled: bool,     // Set by $4017 D6
    pub open_bus: u8,      // This open bus gets set during any write to PPU registers
    clock_rate: f32,       // Same as CPU but is affected by speed changes
    cycle: usize,          // Current APU cycle
    samples: Vec<f32>,     // Buffer of samples
    pub frame_sequencer: FrameSequencer,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    enabled: [bool; 5],
    pub dmc: Dmc,
    filters: [FilterType; 3],
    pulse_table: [f32; Self::PULSE_TABLE_SIZE],
    tnd_table: [f32; Self::TND_TABLE_SIZE],
}

impl Apu {
    const PULSE_TABLE_SIZE: usize = 31;
    const TND_TABLE_SIZE: usize = 203;

    pub fn new() -> Self {
        let mut apu = Self {
            irq_pending: false,
            irq_enabled: false,
            open_bus: 0u8,
            clock_rate: CPU_CLOCK_RATE,
            cycle: 0usize,
            samples: Vec::with_capacity(SAMPLE_BUFFER_SIZE),
            frame_sequencer: FrameSequencer::new(),
            pulse1: Pulse::new(PulseChannel::One),
            pulse2: Pulse::new(PulseChannel::Two),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: Dmc::new(),
            enabled: [true; 5],
            filters: [
                FilterType::HiPassFilter(HiPassFilter::new(90.0, SAMPLE_RATE)),
                FilterType::HiPassFilter(HiPassFilter::new(440.0, SAMPLE_RATE)),
                FilterType::LoPassFilter(LoPassFilter::new(14_000.0, SAMPLE_RATE)),
            ],
            pulse_table: [0f32; Self::PULSE_TABLE_SIZE],
            tnd_table: [0f32; Self::TND_TABLE_SIZE],
        };
        for i in 1..Self::PULSE_TABLE_SIZE {
            apu.pulse_table[i] = 95.52 / (8_128.0 / (i as f32) + 100.0);
        }
        for i in 1..Self::TND_TABLE_SIZE {
            apu.tnd_table[i] = 163.67 / (24_329.0 / (i as f32) + 100.0);
        }
        apu
    }

    pub fn load_mapper(&mut self, mapper: &mut MapperType) {
        self.dmc.mapper = &mut *mapper as *mut MapperType;
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn clear_samples(&mut self) {
        self.samples.clear();
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.clock_rate = CPU_CLOCK_RATE * speed;
    }

    pub fn toggle_pulse1(&mut self) {
        self.enabled[0] = !self.enabled[0];
    }

    pub fn toggle_pulse2(&mut self) {
        self.enabled[1] = !self.enabled[1];
    }

    pub fn toggle_triangle(&mut self) {
        self.enabled[2] = !self.enabled[2];
    }

    pub fn toggle_noise(&mut self) {
        self.enabled[3] = !self.enabled[3];
    }

    pub fn toggle_dmc(&mut self) {
        self.enabled[4] = !self.enabled[4];
    }

    // Counts CPU clocks and determines when to clock quarter/half frames
    // counter is in CPU clocks to avoid APU half-frames
    fn clock_frame_sequencer(&mut self) {
        let clock = self.frame_sequencer.clock();
        match self.frame_sequencer.mode {
            FcMode::Step4 => {
                // mode 0: 4-step  effective rate (approx)
                // ---------------------------------------
                //     - - - f      60 Hz
                //     - l - l     120 Hz
                //     e e e e     240 Hz
                match clock {
                    1 | 3 => self.clock_quarter_frame(),
                    2 => {
                        self.clock_quarter_frame();
                        self.clock_half_frame();
                    }
                    4 => {
                        self.clock_quarter_frame();
                        self.clock_half_frame();
                        if self.irq_enabled {
                            self.irq_pending = true;
                        }
                    }
                    _ => (),
                }
            }
            FcMode::Step5 => {
                // mode 1: 5-step  effective rate (approx)
                // ---------------------------------------
                // - - - - -   (interrupt flag never set)
                // l - l - -    96 Hz
                // e e e e -   192 Hz
                match clock {
                    1 | 3 => {
                        self.clock_quarter_frame();
                        self.clock_half_frame();
                    }
                    2 | 4 => {
                        self.clock_quarter_frame();
                    }
                    _ => (),
                }
            }
        }
    }

    fn clock_quarter_frame(&mut self) {
        self.pulse1.clock_quarter_frame();
        self.pulse2.clock_quarter_frame();
        self.triangle.clock_quarter_frame();
        self.noise.clock_quarter_frame();
    }

    fn clock_half_frame(&mut self) {
        self.pulse1.clock_half_frame();
        self.pulse2.clock_half_frame();
        self.triangle.clock_half_frame();
        self.noise.clock_half_frame();
    }

    fn output(&mut self) -> f32 {
        let pulse1 = if self.enabled[0] {
            self.pulse1.output()
        } else {
            0.0
        };
        let pulse2 = if self.enabled[1] {
            self.pulse2.output()
        } else {
            0.0
        };
        let triangle = if self.enabled[2] {
            self.triangle.output()
        } else {
            0.0
        };
        let noise = if self.enabled[3] {
            self.noise.output()
        } else {
            0.0
        };
        let dmc = if self.enabled[4] {
            self.dmc.output()
        } else {
            0.0
        };

        let pulse_out = self.pulse_table[(pulse1 + pulse2) as usize % 31];
        let tnd_out = self.tnd_table[(3.5 * triangle + 2.0 * noise + dmc) as usize % 203];
        2.0 * (pulse_out + tnd_out)
    }

    // $4015 READ
    fn read_status(&mut self) -> u8 {
        let val = self.peek_status();
        self.irq_pending = false;
        val
    }

    fn peek_status(&self) -> u8 {
        let mut status = 0;
        if self.pulse1.length.counter > 0 {
            status |= 0x01;
        }
        if self.pulse2.length.counter > 0 {
            status |= 0x02;
        }
        if self.triangle.length.counter > 0 {
            status |= 0x04;
        }
        if self.noise.length.counter > 0 {
            status |= 0x08;
        }
        if self.dmc.length > 0 {
            status |= 0x10;
        }
        if self.irq_pending {
            status |= 0x40;
        }
        if self.dmc.irq_pending {
            status |= 0x80;
        }
        status
    }

    // $4015 WRITE
    fn write_status(&mut self, val: u8) {
        self.pulse1.enabled = val & 1 == 1;
        if !self.pulse1.enabled {
            self.pulse1.length.counter = 0;
        }
        self.pulse2.enabled = (val >> 1) & 1 == 1;
        if !self.pulse2.enabled {
            self.pulse2.length.counter = 0;
        }
        self.triangle.enabled = (val >> 2) & 1 == 1;
        if !self.triangle.enabled {
            self.triangle.length.counter = 0;
        }
        self.noise.enabled = (val >> 3) & 1 == 1;
        if !self.noise.enabled {
            self.noise.length.counter = 0;
        }
        let dmc_enabled = (val >> 4) & 1 == 1;
        if dmc_enabled {
            if self.dmc.length == 0 {
                self.dmc.length = self.dmc.length_load;
                self.dmc.addr = self.dmc.addr_load;
            }
        } else {
            self.dmc.length = 0;
        }
        self.dmc.irq_pending = false;
    }

    // $4017 APU frame counter
    fn write_frame_counter(&mut self, val: u8) {
        self.frame_sequencer.reload(val);
        if self.cycle % 2 == 0 {
            self.frame_sequencer.divider.counter += 1.0;
        } else {
            self.frame_sequencer.divider.counter += 2.0;
        }
        // Clock Step5 immediately
        if self.frame_sequencer.mode == FcMode::Step5 {
            self.clock_quarter_frame();
            self.clock_half_frame();
        }
        self.irq_enabled = val & 0x40 == 0x00; // D6
        if !self.irq_enabled {
            self.irq_pending = false;
        }
    }
}

impl Clocked for Apu {
    fn clock(&mut self) -> usize {
        if self.cycle % 2 == 0 {
            self.pulse1.clock();
            self.pulse2.clock();
            self.noise.clock();
            self.dmc.clock();
        }
        self.triangle.clock();
        // Technically only clocks every 2 CPU cycles, but due
        // to half-cycle timings, we clock every cycle
        self.clock_frame_sequencer();

        if self.cycle % (self.clock_rate / SAMPLE_RATE) as usize == 0 {
            let mut sample = self.output();
            for filter in self.filters.iter_mut() {
                sample = filter.process(sample);
            }
            self.samples.push(sample);
        }
        self.cycle += 1;
        1
    }
}

impl MemRead for Apu {
    fn read(&mut self, addr: u16) -> u8 {
        if addr == 0x4015 {
            let val = self.read_status();
            self.open_bus = val;
            val
        } else {
            self.open_bus
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        if addr == 0x4015 {
            self.peek_status()
        } else {
            self.open_bus
        }
    }
}

impl MemWrite for Apu {
    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        match addr {
            0x4000 => self.pulse1.write_control(val),
            0x4001 => self.pulse1.write_sweep(val),
            0x4002 => self.pulse1.write_timer_lo(val),
            0x4003 => self.pulse1.write_timer_hi(val),
            0x4004 => self.pulse2.write_control(val),
            0x4005 => self.pulse2.write_sweep(val),
            0x4006 => self.pulse2.write_timer_lo(val),
            0x4007 => self.pulse2.write_timer_hi(val),
            0x4008 => self.triangle.write_linear_counter(val),
            0x400A => self.triangle.write_timer_lo(val),
            0x400B => self.triangle.write_timer_hi(val),
            0x400C => self.noise.write_control(val),
            0x400E => self.noise.write_timer(val),
            0x400F => self.noise.write_length(val),
            0x4010 => self.dmc.write_timer(val),
            0x4011 => self.dmc.write_output(val),
            0x4012 => self.dmc.write_addr_load(val),
            0x4013 => self.dmc.write_length(val),
            0x4015 => self.write_status(val),
            0x4017 => self.write_frame_counter(val),
            _ => (),
        }
    }
}

impl Powered for Apu {
    fn reset(&mut self) {
        self.cycle = 0;
        self.samples.clear();
        self.irq_pending = false;
        self.irq_enabled = false;
        self.frame_sequencer = FrameSequencer::new();
        self.pulse1.reset();
        self.pulse2.reset();
        self.triangle.reset();
        self.noise.reset();
        self.dmc.reset();
    }
}

impl Savable for Apu {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.irq_pending.save(fh)?;
        self.irq_enabled.save(fh)?;
        self.open_bus.save(fh)?;
        // Ignore clock_rate
        self.cycle.save(fh)?;
        // Ignore samples
        self.frame_sequencer.save(fh)?;
        self.pulse1.save(fh)?;
        self.pulse2.save(fh)?;
        self.triangle.save(fh)?;
        self.noise.save(fh)?;
        self.dmc.save(fh)?;
        // Ignore
        // log_level
        // hifilters
        // lofilters
        // pulse_table
        // tnd_Table
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.irq_pending.load(fh)?;
        self.irq_enabled.load(fh)?;
        self.open_bus.load(fh)?;
        self.cycle.load(fh)?;
        self.frame_sequencer.load(fh)?;
        self.pulse1.load(fh)?;
        self.pulse2.load(fh)?;
        self.triangle.load(fh)?;
        self.noise.load(fh)?;
        self.dmc.load(fh)?;
        Ok(())
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Apu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        write!(f, "APU {{ cyc: {} }}", self.cycle)
    }
}

#[cfg(test)]
mod tests {}
