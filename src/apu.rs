//! Audio Processing Unit
//!
//! <https://wiki.nesdev.com/w/index.php/APU>

use crate::{
    apu::pulse::OutputFreq,
    cart::Cart,
    common::{Clocked, NesFormat, Powered},
    cpu::CPU_CLOCK_RATE,
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

pub const SAMPLE_RATE: f32 = 44_100.0; // in Hz
const SAMPLE_BUFFER_SIZE: usize = 1024;

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
    cycle: usize,    // Current APU cycle
    clock_rate: f32, // Same as CPU but is affected by speed changes
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
    sample_timer: f32,
    sample_rate: f32,
    pub(crate) open_bus: u8, // This open bus gets set during any write to APU registers
}

impl Apu {
    pub fn new(nes_format: NesFormat) -> Self {
        Self {
            cycle: 0usize,
            clock_rate: CPU_CLOCK_RATE,
            nes_format,
            irq_pending: false,
            irq_disabled: false,
            samples: Vec::with_capacity(SAMPLE_BUFFER_SIZE),
            frame_counter: FrameCounter::new(nes_format),
            pulse1: Pulse::new(PulseChannel::One, OutputFreq::Default),
            pulse2: Pulse::new(PulseChannel::Two, OutputFreq::Default),
            triangle: Triangle::new(),
            noise: Noise::new(nes_format),
            dmc: Dmc::new(nes_format),
            cart: std::ptr::null_mut(),
            enabled: [true; 5],
            sample_timer: 0.0,
            sample_rate: CPU_CLOCK_RATE / SAMPLE_RATE,
            open_bus: 0u8,
        }
    }

    pub fn set_nes_format(&mut self, nes_format: NesFormat) {
        self.nes_format = nes_format;
        self.frame_counter.set_nes_format(nes_format);
        self.dmc.set_nes_format(nes_format);
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
        pulse_out + tnd_out
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
            .field("clock_rate", &self.clock_rate)
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
    #![allow(clippy::unreadable_literal)]
    use crate::{
        common::{tests::compare, NesFormat, Powered},
        test_roms, test_roms_adv,
    };

    test_roms!("apu", {
        (clock_jitter, 15, 11142254853534581794),
        (dmc_basics, 25, 4777243056264901558),
        (dmc_dma_2007_read, 25, 9760800171373506878),
        (dmc_dma_2007_write, 30, 6819750118289511461),
        (dmc_dma_4016_read, 20, 17611075533891223752),
        (dmc_dma_double_2007_read, 20, 10498985860445899032),
        (dmc_dma_read_write_2007, 24, 17262164619652057735),
        (dmc_rates, 27, 11063982786335661106),
        (dpcmletterbox, 10, 1985156316546052267),
        (irq_flag, 16, 11142254853534581794),
        (irq_flag_timing, 100, 0, "fails $04"),
        (irq_timing, 15, 11142254853534581794),
        (jitter, 18, 1036648701261398994),
        (len_ctr, 25, 11142254853534581794),
        (len_halt_timing, 100, 0, "fails $03"),
        (len_reload_timing, 100, 0, "fails $04"),
        (len_table, 10, 11142254853534581794),
        (len_timing, 100, 0, "Channel: 0 second length of mode 0 is too soon"),
        (len_timing_mode0, 100, 0, "fails $04"),
        (len_timing_mode1, 100, 0, "fails $05"),
        (reset_len_ctrs_enabled, 100, 0, "At power, length counters should be enabled, #2"),
        (reset_timing, 10, 11142254853534581794),
        (test_1, 10, 2319187644663237904),
        (test_2, 10, 2319187644663237904),
        (test_3, 100, 0, "fails"),
        (test_4, 100, 0, "fails"),
        (test_5, 10, 2319187644663237904),
        (test_6, 10, 2319187644663237904),
        (test_7, 100, 0, "fails"),
        (test_8, 100, 0, "fails"),
        (test_9, 100, 0, "fails"),
        (test_10, 100, 0, "fails"),
        (pal_clock_jitter, 100, 0, "fails"),
        (pal_irq_flag_timing, 100, 0, "fails"),
        (pal_len_halt_timing, 100, 0, "fails"),
        (pal_len_reload_timing, 100, 0, "fails"),
        (pal_len_timing_mode0, 100, 0, "fails"),
        (pal_len_timing_mode1, 100, 0, "fails"),
    });

    test_roms_adv!("apu", {
        (reset_4015_cleared, 15, |frame, deck| match frame {
            10 => deck.reset(),
            17 => compare(116295277903678038, deck, "reset_4015_cleared"),
            _ => (),
        }),
        (reset_4017_timing, 37, |frame, deck| match frame {
            20 => deck.reset(),
            39 => compare(14926929218207596099, deck, "reset_4017_timing"),
            _ => (),
        }),
        (reset_4017_written, 0, |frame, deck| match frame {
            17 => deck.reset(),
            32 => deck.reset(),
            47 => compare(12593305160591345698, deck, "reset_4017_written"),
            _ => ()
        }),
        (reset_irq_flag_cleared, 16, |frame, deck| match frame {
            11 => deck.reset(),
            18 => compare(13991247418321945900, deck, "reset_irq_flag_cleared"),
            _ => (),
        }),
        (reset_works_immediately, 18, |frame, deck| match frame {
            15 => deck.reset(),
            21 => compare(1786657150847637076, deck, "reset_works_immediately"),
            _ => (),
        }),
        (pal_irq_flag, 15, |frame, deck| match frame {
            0 => deck.set_nes_format(NesFormat::Pal),
            15 => compare(1476332058693542633, deck, "pal_irq_flag"),
            _ => (),
        }),
        (pal_irq_timing, 15,  |frame, deck| match frame {
            0 => deck.set_nes_format(NesFormat::Pal),
            15 => compare(4151069387652564427, deck, "pal_irq_timing"),
            _ => (),
        }),
        (pal_len_ctr, 25,  |frame, deck| match frame {
            0 => deck.set_nes_format(NesFormat::Pal),
            25 => compare(8844976523644404419, deck, "pal_len_ctr"),
            _ => (),
        }),
        (pal_len_table, 15,  |frame, deck| match frame {
            0 => deck.set_nes_format(NesFormat::Pal),
            15 => compare(16541936846276814327, deck, "pal_len_table"),
            _ => (),
        }),
    });
}
