//! Audio Processing Unit
//!
//! <https://wiki.nesdev.com/w/index.php/APU>

use crate::{
    apu::pulse::OutputFreq,
    cart::Cart,
    common::{Clocked, NesFormat, Powered},
    cpu::Cpu,
    mapper::Mapper,
    memory::{MemRead, MemWrite},
};
use dmc::Dmc;
use frame_counter::{FcMode, FrameCounter};
use lazy_static::lazy_static;
use noise::Noise;
use pulse::{Pulse, PulseChannel};
use serde::{Deserialize, Serialize};
use std::fmt;
use triangle::Triangle;

const PULSE_TABLE_SIZE: usize = 31;
const TND_TABLE_SIZE: usize = 203;

lazy_static! {
    static ref PULSE_TABLE: [f32; PULSE_TABLE_SIZE] = {
        let mut pulse_table = [0.0; PULSE_TABLE_SIZE];
        for (i, val) in pulse_table.iter_mut().enumerate().skip(1) {
            *val = 95.52 / (8_128.0 / (i as f32) + 100.0);
        }
        pulse_table
    };
    static ref TND_TABLE: [f32; TND_TABLE_SIZE] = {
        let mut tnd_table = [0.0; TND_TABLE_SIZE];
        for (i, val) in tnd_table.iter_mut().enumerate().skip(1) {
            *val = 163.67 / (24_329.0 / (i as f32) + 100.0);
        }
        tnd_table
    };
}

pub mod dmc;
pub mod noise;
pub mod pulse;
pub mod triangle;

mod envelope;
mod frame_counter;

/// A given APU audio channel.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum AudioChannel {
    Pulse1,
    Pulse2,
    Triangle,
    Noise,
    Dmc,
}

/// Audio Processing Unit
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Apu {
    cycle: usize, // Current APU cycle
    nes_format: NesFormat,
    pub(crate) irq_pending: bool, // Set by $4017 if irq_enabled is clear or set during step 4 of Step4 mode
    irq_disabled: bool,           // Set by $4017 D6
    samples: Vec<f32>,            // Buffer of samples
    frame_counter: FrameCounter,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    pub(crate) dmc: Dmc,
    #[serde(skip, default = "std::ptr::null_mut")]
    cart: *mut Cart,
    enabled: [bool; 5],
    pub(crate) open_bus: u8, // This open bus gets set during any write to APU registers
}

impl Apu {
    pub fn new(nes_format: NesFormat) -> Self {
        Self {
            cycle: 0,
            nes_format,
            irq_pending: false,
            irq_disabled: false,
            // Start with ~20ms of audio capacity
            samples: Vec::with_capacity((Cpu::clock_rate(nes_format) * 0.02) as usize),
            frame_counter: FrameCounter::new(nes_format),
            pulse1: Pulse::new(PulseChannel::One, OutputFreq::Default),
            pulse2: Pulse::new(PulseChannel::Two, OutputFreq::Default),
            triangle: Triangle::new(),
            noise: Noise::new(nes_format),
            dmc: Dmc::new(nes_format),
            cart: std::ptr::null_mut(),
            enabled: [true; 5],
            open_bus: 0x00,
        }
    }

    pub fn set_nes_format(&mut self, nes_format: NesFormat) {
        self.nes_format = nes_format;
        self.frame_counter.set_nes_format(nes_format);
        self.dmc.set_nes_format(nes_format);
    }

    #[inline]
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        Cpu::clock_rate(self.nes_format)
    }

    #[inline]
    #[must_use]
    pub fn samples(&mut self) -> &mut [f32] {
        &mut self.samples
    }

    #[inline]
    pub fn clear_samples(&mut self) {
        self.samples.clear();
    }

    #[inline]
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
    fn clock_frame_counter(&mut self) {
        let clock = self.frame_counter.clock();

        if self.frame_counter.mode == FcMode::Step4
            && !self.irq_disabled
            && self.frame_counter.step >= 4
        {
            self.irq_pending = true;
        }

        // mode 0: 4-step  effective rate (approx)
        // ---------------------------------------
        // - - - f f f      60 Hz
        // - l - - l -     120 Hz
        // e e e - e -     240 Hz
        //
        // mode 1: 5-step  effective rate (approx)
        // ---------------------------------------
        // - - - - - -     (interrupt flag never set)
        // - l - - l -     96 Hz
        // e e e - e -     192 Hz
        match clock {
            1 | 3 => {
                self.clock_quarter_frame();
            }
            2 | 5 => {
                self.clock_quarter_frame();
                self.clock_half_frame();
            }
            _ => (),
        }

        // Clock Step5 immediately
        if self.frame_counter.update() && self.frame_counter.mode == FcMode::Step5 {
            self.clock_quarter_frame();
            self.clock_half_frame();
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

    #[inline]
    fn output(&mut self) {
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
        let (pulse, dmc) = if let Mapper::Exrom(ref exrom) = self.cart().mapper {
            let pulse3 = exrom.pulse1.output();
            let pulse4 = exrom.pulse2.output();
            let dmc2 = exrom.dmc.output();
            (pulse1 + pulse2 + pulse3 + pulse4, dmc + dmc2)
        } else {
            (pulse1 + pulse2, dmc)
        };
        let pulse_out = PULSE_TABLE[pulse as usize % 31];
        let tnd_out = TND_TABLE[(3.0f32.mul_add(triangle, 2.0 * noise) + dmc) as usize % 203];
        let sample = pulse_out + tnd_out;
        self.samples.push(sample);
    }

    // $4015 READ
    #[inline]
    fn read_status(&mut self) -> u8 {
        let val = self.peek_status();
        self.irq_pending = false;
        val
    }

    #[inline]
    #[must_use]
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
        self.frame_counter.write(val, self.cycle);
        self.irq_disabled = val & 0x40 == 0x40; // D6
        if self.irq_disabled {
            self.irq_pending = false;
        }
    }

    #[inline]
    pub fn load_cart(&mut self, cart: &mut Cart) {
        self.cart = cart;
    }

    #[allow(clippy::missing_const_for_fn)]
    #[inline]
    pub fn cart(&self) -> &Cart {
        unsafe { &*self.cart }
    }

    #[inline]
    pub fn cart_mut(&mut self) -> &mut Cart {
        unsafe { &mut *self.cart }
    }
}

impl Clocked for Apu {
    #[inline]
    fn clock(&mut self) -> usize {
        self.dmc.check_pending_dma();
        if self.cycle & 0x01 == 0x00 {
            self.pulse1.clock();
            self.pulse2.clock();
            self.noise.clock();
            self.dmc.clock();
        }
        self.triangle.clock();
        // Technically only clocks every 2 CPU cycles, but due
        // to half-cycle timings, we clock every cycle
        self.clock_frame_counter();
        self.output();
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
        self.irq_disabled = false;
        self.frame_counter.reset();
        self.pulse1.reset();
        self.pulse2.reset();
        self.triangle.reset();
        self.noise.reset();
        self.dmc.reset();
    }

    fn power_cycle(&mut self) {
        self.frame_counter.power_cycle();
        self.reset();
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new(NesFormat::default())
    }
}

impl fmt::Debug for Apu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Apu")
            .field("irq_pending", &self.irq_pending)
            .field("irq_disabled", &self.irq_disabled)
            .field("open_bus", &format_args!("${:02X}", &self.open_bus))
            .field("samples len", &self.samples.len())
            .field("frame_counter", &self.frame_counter)
            .field("pulse1", &self.pulse1)
            .field("pulse2", &self.pulse2)
            .field("triangle", &self.triangle)
            .field("noise", &self.noise)
            .field("dmc", &self.dmc)
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Sweep {
    pub(crate) enabled: bool,
    pub(crate) reload: bool,
    pub(crate) negate: bool, // Treats PulseChannel 1 differently than PulseChannel 2
    pub(crate) timer: u8,    // counter reload value
    pub(crate) counter: u8,  // current timer value
    pub(crate) shift: u8,
}

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
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

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
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
mod tests {
    use crate::test_roms;

    test_roms!(
        "test_roms/apu",
        clock_jitter,
        dmc_basics,
        dmc_dma_2007_read,
        dmc_dma_2007_write,
        dmc_dma_4016_read,
        dmc_dma_double_2007_read,
        dmc_dma_read_write_2007,
        dmc_rates,
        dpcmletterbox,
        irq_flag,
        #[ignore = "fails $04"]
        irq_flag_timing,
        irq_timing,
        jitter,
        len_ctr,
        #[ignore = "fails $03"]
        len_halt_timing,
        #[ignore = "fails $04"]
        len_reload_timing,
        len_table,
        #[ignore = "Channel: 0 second length of mode 0 is too soon"]
        len_timing,
        #[ignore = "fails $04"]
        len_timing_mode0,
        #[ignore = "fails $05"]
        len_timing_mode1,
        reset_4015_cleared,
        reset_4017_timing,
        reset_4017_written,
        reset_irq_flag_cleared,
        #[ignore = "At power, length counters should be enabled, #2"]
        reset_len_ctrs_enabled,
        reset_timing,
        reset_works_immediately,
        test_1,
        test_2,
        #[ignore = "todo"]
        test_3,
        #[ignore = "todo"]
        test_4,
        test_5,
        test_6,
        #[ignore = "todo"]
        test_7,
        #[ignore = "todo"]
        test_8,
        #[ignore = "todo"]
        test_9,
        #[ignore = "todo"]
        test_10,
        #[ignore = "todo"]
        pal_clock_jitter,
        pal_irq_flag,
        #[ignore = "todo"]
        pal_irq_flag_timing,
        #[ignore = "todo"]
        pal_irq_timing,
        pal_len_ctr,
        #[ignore = "todo"]
        pal_len_halt_timing,
        #[ignore = "todo"]
        pal_len_reload_timing,
        pal_len_table,
        #[ignore = "todo"]
        pal_len_timing_mode0,
        #[ignore = "todo"]
        pal_len_timing_mode1,
    );
}
