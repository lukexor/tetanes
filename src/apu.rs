//! Audio Processing Unit
//!
//! <https://wiki.nesdev.com/w/index.php/APU>

use crate::{
    common::{Clocked, Powered},
    cpu::CPU_CLOCK_RATE,
    memory::{MemRead, MemWrite},
};
use dmc::Dmc;
use frame_sequencer::{FcMode, FrameSequencer};
use noise::Noise;
use pulse::{Pulse, PulseChannel};
use std::fmt;
use triangle::Triangle;

pub const SAMPLE_RATE: f32 = 44_100.0; // in Hz
const SAMPLE_BUFFER_SIZE: usize = 1024;

pub mod dmc;
pub mod noise;
pub mod pulse;
pub mod triangle;

mod envelope;
mod frame_sequencer;

/// A given APU audio channel.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[must_use]
pub enum AudioChannel {
    Pulse1,
    Pulse2,
    Triangle,
    Noise,
    Dmc,
}

/// Audio Processing Unit
#[derive(Clone)]
#[must_use]
pub struct Apu {
    pub(crate) irq_pending: bool, // Set by $4017 if irq_enabled is clear or set during step 4 of Step4 mode
    irq_enabled: bool,            // Set by $4017 D6
    pub(crate) open_bus: u8,      // This open bus gets set during any write to PPU registers
    clock_rate: f32,              // Same as CPU but is affected by speed changes
    cycle: usize,                 // Current APU cycle
    samples: Vec<f32>,            // Buffer of samples
    frame_sequencer: FrameSequencer,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    pub(crate) dmc: Dmc,
    enabled: [bool; 5],
    sample_timer: f32,
    sample_rate: f32,
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
            sample_timer: 0.0,
            sample_rate: CPU_CLOCK_RATE / SAMPLE_RATE,
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

    #[must_use]
    #[inline]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    #[inline]
    pub fn clear_samples(&mut self) {
        self.samples.clear();
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.clock_rate = CPU_CLOCK_RATE * speed;
        self.sample_rate = self.clock_rate / SAMPLE_RATE;
    }

    #[must_use]
    pub const fn channel_enabled(&self, channel: AudioChannel) -> bool {
        self.enabled[channel as usize]
    }

    pub fn toggle_channel(&mut self, channel: AudioChannel) {
        self.enabled[channel as usize] = !self.enabled[channel as usize];
    }

    // Counts CPU clocks and determines when to clock quarter/half frames
    // counter is in CPU clocks to avoid APU half-frames
    #[inline]
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

    #[inline]
    fn clock_quarter_frame(&mut self) {
        self.pulse1.clock_quarter_frame();
        self.pulse2.clock_quarter_frame();
        self.triangle.clock_quarter_frame();
        self.noise.clock_quarter_frame();
    }

    #[inline]
    fn clock_half_frame(&mut self) {
        self.pulse1.clock_half_frame();
        self.pulse2.clock_half_frame();
        self.triangle.clock_half_frame();
        self.noise.clock_half_frame();
    }

    #[must_use]
    #[inline]
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
        let tnd_out = self.tnd_table[(3.5f32.mul_add(triangle, 2.0 * noise) + dmc) as usize % 203];
        2.0 * (pulse_out + tnd_out)
    }

    // $4015 READ
    #[inline]
    fn read_status(&mut self) -> u8 {
        let val = self.peek_status();
        self.irq_pending = false;
        val
    }

    #[must_use]
    #[inline]
    const fn peek_status(&self) -> u8 {
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
    #[inline]
    fn write_status(&mut self, val: u8) {
        self.pulse1.set_enabled(val & 0x01 == 0x01);
        self.pulse2.set_enabled(val & 0x02 == 0x02);
        self.triangle.set_enabled(val & 0x04 == 0x04);
        self.noise.set_enabled(val & 0x08 == 0x08);
        self.dmc.set_enabled(val & 0x10 == 0x10, self.cycle);
    }

    // $4017 APU frame counter
    #[inline]
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

#[cfg(test)]
impl Apu {
    #[inline]
    pub(crate) fn frame_sequencer(&self) -> &FrameSequencer {
        &self.frame_sequencer
    }
}

impl Clocked for Apu {
    fn clock(&mut self) -> usize {
        if self.cycle & 0x01 == 0x00 {
            self.pulse1.clock();
            self.pulse2.clock();
            self.noise.clock();
            self.dmc.clock();
        }
        self.triangle.clock();
        // Technically only clocks every 2 CPU cycles, but due
        // to half-cycle timings, we clock every cycle
        self.clock_frame_sequencer();

        self.sample_timer += 1.0;
        if self.sample_timer > self.sample_rate {
            let sample = self.output();
            self.samples.push(sample);
            self.sample_timer -= self.sample_rate;
        }
        self.cycle += 1;
        1
    }
}

impl MemRead for Apu {
    #[inline]
    fn read(&mut self, addr: u16) -> u8 {
        if addr == 0x4015 {
            let val = self.read_status();
            self.open_bus = val;
            val
        } else {
            self.open_bus
        }
    }

    #[inline]
    fn peek(&self, addr: u16) -> u8 {
        if addr == 0x4015 {
            self.peek_status()
        } else {
            self.open_bus
        }
    }
}

impl MemWrite for Apu {
    #[inline]
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

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Apu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Apu")
            .field("irq_pending", &self.irq_pending)
            .field("irq_enabled", &self.irq_enabled)
            .field("open_bus", &format_args!("${:02X}", &self.open_bus))
            .field("clock_rate", &self.clock_rate)
            .field("samples len", &self.samples.len())
            .field("frame_sequencer", &self.frame_sequencer)
            .field("pulse1", &self.pulse1)
            .field("pulse2", &self.pulse2)
            .field("triangle", &self.triangle)
            .field("noise", &self.noise)
            .field("dmc", &self.dmc)
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Default, Debug, Clone)]
#[must_use]
pub(crate) struct Sequencer {
    pub(crate) step: usize,
    pub(crate) length: usize,
}

impl Sequencer {
    pub(crate) const fn new(length: usize) -> Self {
        Self { step: 1, length }
    }
}

impl Clocked for Sequencer {
    #[inline]
    fn clock(&mut self) -> usize {
        let clock = self.step;
        self.step += 1;
        if self.step > self.length {
            self.step = 1;
        }
        clock
    }
}

impl Powered for Sequencer {
    fn reset(&mut self) {
        self.step = 1;
    }
}

#[derive(Default, Debug, Clone)]
#[must_use]
pub(crate) struct Divider {
    pub(crate) counter: f32,
    pub(crate) period: f32,
}

impl Divider {
    pub(super) const fn new(period: f32) -> Self {
        Self {
            counter: period,
            period,
        }
    }
}

impl Clocked for Divider {
    #[must_use]
    fn clock(&mut self) -> usize {
        if self.counter > 0.0 {
            self.counter -= 1.0;
        }
        if self.counter <= 0.0 {
            // Reset and output a clock
            self.counter += self.period;
            1
        } else {
            0
        }
    }
}

impl Powered for Divider {
    fn reset(&mut self) {
        self.counter = self.period;
    }
}

#[derive(Default, Debug, Clone)]
pub(crate) struct Sweep {
    pub(crate) enabled: bool,
    pub(crate) reload: bool,
    pub(crate) negate: bool, // Treats PulseChannel 1 differently than PulseChannel 2
    pub(crate) timer: u8,    // counter reload value
    pub(crate) counter: u8,  // current timer value
    pub(crate) shift: u8,
}

#[derive(Default, Debug, Copy, Clone)]
#[must_use]
pub struct LengthCounter {
    pub enabled: bool,
    pub counter: u8, // Entry into LENGTH_TABLE
}

impl LengthCounter {
    const LENGTH_TABLE: [u8; 32] = [
        10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96,
        22, 192, 24, 72, 26, 16, 28, 32, 30,
    ];

    pub const fn new() -> Self {
        Self {
            enabled: false,
            counter: 0u8,
        }
    }

    #[inline]
    pub fn load_value(&mut self, val: u8) {
        self.counter = Self::LENGTH_TABLE[(val >> 3) as usize]; // D7..D3
    }

    #[inline]
    pub fn write_control(&mut self, val: u8) {
        self.enabled = (val >> 5) & 1 == 0; // !D5
    }
}

impl Clocked for LengthCounter {
    #[inline]
    fn clock(&mut self) -> usize {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
            1
        } else {
            0
        }
    }
}

#[derive(Default, Debug, Clone)]
#[must_use]
pub(crate) struct LinearCounter {
    pub(crate) reload: bool,
    pub(crate) control: bool,
    pub(crate) load: u8,
    pub(crate) counter: u8,
}

impl LinearCounter {
    pub(crate) const fn new() -> Self {
        Self {
            reload: false,
            control: false,
            load: 0u8,
            counter: 0u8,
        }
    }

    #[inline]
    pub(crate) fn load_value(&mut self, val: u8) {
        self.load = val >> 1; // D6..D0
    }
}

#[cfg(test)]
mod tests {}
