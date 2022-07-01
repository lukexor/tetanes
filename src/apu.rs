use crate::{
    apu::{
        dmc::Dmc,
        frame_counter::{FcMode, FrameCounter},
        noise::Noise,
        pulse::{OutputFreq, Pulse, PulseChannel},
        triangle::Triangle,
    },
    audio::Audio,
    common::{Clock, Kind, NesRegion, Regional, Reset},
    cpu::Irq,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

pub mod dmc;
pub mod noise;
pub mod pulse;
pub mod triangle;

pub mod envelope;
pub mod frame_counter;
pub mod length_counter;
pub mod linear_counter;
pub mod sweep;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Channel {
    Pulse1,
    Pulse2,
    Triangle,
    Noise,
    Dmc,
}

pub trait ApuRegisters {
    fn write_ctrl(&mut self, channel: Channel, val: u8);
    fn write_sweep(&mut self, channel: Channel, val: u8);
    fn write_timer_lo(&mut self, channel: Channel, val: u8);
    fn write_timer_hi(&mut self, channel: Channel, val: u8);
    fn write_linear_counter(&mut self, channel: Channel, val: u8);
    fn write_length(&mut self, channel: Channel, val: u8);
    fn write_output(&mut self, channel: Channel, val: u8);
    fn write_addr_load(&mut self, channel: Channel, val: u8);
    fn read_status(&mut self) -> u8;
    fn peek_status(&self) -> u8;
    fn write_status(&mut self, val: u8);
    fn write_frame_counter(&mut self, val: u8);
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Apu {
    cycle: usize,
    region: NesRegion,
    irq_pending: bool, // Set by $4017 if irq_enabled is clear or set during step 4 of Step4 mode
    irq_disabled: bool, // Set by $4017 D6
    frame_counter: FrameCounter,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            cycle: 0,
            region: NesRegion::default(),
            irq_pending: false,
            irq_disabled: false,
            frame_counter: FrameCounter::new(),
            pulse1: Pulse::new(PulseChannel::One, OutputFreq::Default),
            pulse2: Pulse::new(PulseChannel::Two, OutputFreq::Default),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: Dmc::new(),
        }
    }

    #[inline]
    #[must_use]
    pub const fn channel_enabled(&self, channel: Channel) -> bool {
        match channel {
            Channel::Pulse1 => self.pulse1.silent(),
            Channel::Pulse2 => self.pulse1.silent(),
            Channel::Triangle => self.triangle.silent(),
            Channel::Noise => self.noise.silent(),
            Channel::Dmc => self.dmc.silent(),
        }
    }

    #[inline]
    pub fn toggle_channel(&mut self, channel: Channel) {
        match channel {
            Channel::Pulse1 => self.pulse1.toggle_silent(),
            Channel::Pulse2 => self.pulse1.toggle_silent(),
            Channel::Triangle => self.triangle.toggle_silent(),
            Channel::Noise => self.noise.toggle_silent(),
            Channel::Dmc => self.dmc.toggle_silent(),
        }
    }

    #[inline]
    pub fn irqs_pending(&self) -> Irq {
        let mut irq = Irq::empty();
        irq.set(Irq::FRAME_COUNTER, self.irq_pending);
        irq.set(Irq::DMC, self.dmc.irq_pending());
        irq
    }

    #[inline]
    #[must_use]
    pub fn dmc_dma(&mut self) -> bool {
        self.dmc.dma()
    }

    #[inline]
    #[must_use]
    pub const fn dmc_dma_addr(&self) -> u16 {
        self.dmc.dma_addr()
    }

    #[inline]
    pub fn load_dmc_buffer(&mut self, val: u8) {
        self.dmc.load_buffer(val);
    }

    // Counts CPU clocks and determines when to clock quarter/half frames
    // counter is in CPU clocks to avoid APU half-frames
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
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl ApuRegisters for Apu {
    // $4000 Pulse1, $4004 Pulse2, and $400C Noise Control
    fn write_ctrl(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Pulse1 => self.pulse1.write_ctrl(val),
            Channel::Pulse2 => self.pulse2.write_ctrl(val),
            Channel::Noise => self.noise.write_ctrl(val),
            _ => panic!("{:?} does not have a control register", channel),
        }
    }

    // $4001 Pulse1 and $4005 Pulse2 Sweep
    fn write_sweep(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Pulse1 => self.pulse1.write_sweep(val),
            Channel::Pulse2 => self.pulse2.write_sweep(val),
            _ => panic!("{:?} does not have a sweep register", channel),
        }
    }

    // $4002 Pulse1, $4006 Pulse2, $400A Triangle, $400E Noise, and $4010 DMC Timer Low Byte
    fn write_timer_lo(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Pulse1 => self.pulse1.write_timer_lo(val),
            Channel::Pulse2 => self.pulse2.write_timer_lo(val),
            Channel::Triangle => self.triangle.write_timer_lo(val),
            Channel::Noise => self.noise.write_timer(val),
            Channel::Dmc => self.dmc.write_timer(val),
        }
    }

    // $4003 Pulse1, $4007 Pulse2, and $400B Triangle Timer High Byte
    fn write_timer_hi(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Pulse1 => self.pulse1.write_timer_hi(val),
            Channel::Pulse2 => self.pulse2.write_timer_hi(val),
            Channel::Triangle => self.triangle.write_timer_hi(val),
            _ => panic!("{:?} does not have a timer_hi register", channel),
        }
    }

    // $4008 Triangle Linear Counter
    fn write_linear_counter(&mut self, channel: Channel, val: u8) {
        if channel == Channel::Triangle {
            self.triangle.write_linear_counter(val);
        } else {
            panic!("{:?} does not have a linear_counter register", channel);
        }
    }

    // $400F Noise and $4013 DMC Length
    fn write_length(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Noise => self.noise.write_length(val),
            Channel::Dmc => self.dmc.write_length(val),
            _ => panic!("{:?} does not have a length register", channel),
        }
    }

    // $4011 DMC Output
    fn write_output(&mut self, channel: Channel, val: u8) {
        if channel == Channel::Dmc {
            // Only 7-bits are used
            self.dmc.write_output(val & 0x7F);
        } else {
            panic!("{:?} does not have output register", channel);
        }
    }

    // $4012 DMC Addr Load
    fn write_addr_load(&mut self, channel: Channel, val: u8) {
        if channel == Channel::Dmc {
            self.dmc.write_addr_load(val);
        } else {
            panic!("{:?} does not have addr_load register", channel);
        }
    }

    // $4015 | RW  | APU Status
    //       |   0 | Channel 1, 1 = enable sound
    //       |   1 | Channel 2, 1 = enable sound
    //       |   2 | Channel 3, 1 = enable sound
    //       |   3 | Channel 4, 1 = enable sound
    //       |   4 | Channel 5, 1 = enable sound
    //       | 5-7 | Unused (???)
    fn read_status(&mut self) -> u8 {
        let val = self.peek_status();
        self.irq_pending = false;
        val
    }

    // $4015 | RW  | APU Status
    //       |   0 | Channel 1, 1 = enable sound
    //       |   1 | Channel 2, 1 = enable sound
    //       |   2 | Channel 3, 1 = enable sound
    //       |   3 | Channel 4, 1 = enable sound
    //       |   4 | Channel 5, 1 = enable sound
    //       | 5-7 | Unused (???)
    //
    // Non-mutating version of `read_status`.
    fn peek_status(&self) -> u8 {
        let mut status = 0x00;
        if self.pulse1.length_counter() > 0 {
            status |= 0x01;
        }
        if self.pulse2.length_counter() > 0 {
            status |= 0x02;
        }
        if self.triangle.length_counter() > 0 {
            status |= 0x04;
        }
        if self.noise.length_counter() > 0 {
            status |= 0x08;
        }
        if self.dmc.length() > 0 {
            status |= 0x10;
        }
        if self.irq_pending {
            status |= 0x40;
        }
        if self.dmc.irq_pending() {
            status |= 0x80;
        }
        status
    }

    // $4015 | RW  | APU Status
    //       |   0 | Channel 1, 1 = enable sound
    //       |   1 | Channel 2, 1 = enable sound
    //       |   2 | Channel 3, 1 = enable sound
    //       |   3 | Channel 4, 1 = enable sound
    //       |   4 | Channel 5, 1 = enable sound
    //       | 5-7 | Unused (???)
    fn write_status(&mut self, val: u8) {
        self.pulse1.set_enabled(val & 0x01 == 0x01);
        self.pulse2.set_enabled(val & 0x02 == 0x02);
        self.triangle.set_enabled(val & 0x04 == 0x04);
        self.noise.set_enabled(val & 0x08 == 0x08);
        self.dmc.set_enabled(val & 0x10 == 0x10, self.cycle);
    }

    // $4017 APU Frame Counter
    fn write_frame_counter(&mut self, val: u8) {
        self.frame_counter.write(val, self.cycle);
        self.irq_disabled = val & 0x40 == 0x40; // D6
        if self.irq_disabled {
            self.irq_pending = false;
        }
    }
}

impl Audio for Apu {
    #[must_use]
    fn output(&self) -> f32 {
        let pulse1 = self.pulse1.output();
        let pulse2 = self.pulse2.output();
        let triangle = self.triangle.output();
        let noise = self.noise.output();
        let dmc = self.dmc.output();
        let mut pulse_idx = (pulse1 + pulse2) as usize;
        if pulse_idx > PULSE_TABLE.len() {
            pulse_idx %= PULSE_TABLE.len();
        }
        let mut tnd_idx = (3.0f32.mul_add(triangle, 2.0 * noise) + dmc) as usize;
        if tnd_idx > TND_TABLE.len() {
            tnd_idx %= TND_TABLE.len();
        }
        PULSE_TABLE[pulse_idx] + TND_TABLE[tnd_idx]
    }
}

impl Clock for Apu {
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
        self.cycle += 1;
        1
    }
}

impl Regional for Apu {
    #[inline]
    fn region(&self) -> NesRegion {
        self.region
    }

    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.frame_counter.set_region(region);
        self.noise.set_region(region);
        self.dmc.set_region(region);
    }
}

impl Reset for Apu {
    fn reset(&mut self, kind: Kind) {
        self.cycle = 0;
        self.irq_pending = false;
        self.irq_disabled = false;
        self.frame_counter.reset(kind);
        self.pulse1.reset(kind);
        self.pulse2.reset(kind);
        self.triangle.reset(kind);
        self.noise.reset(kind);
        self.dmc.reset(kind);
    }
}

impl std::fmt::Debug for Apu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Apu")
            .field("cycle", &self.cycle)
            .field("irq_pending", &self.irq_pending)
            .field("irq_disabled", &self.irq_disabled)
            .field("frame_counter", &self.frame_counter)
            .field("pulse1", &self.pulse1)
            .field("pulse2", &self.pulse2)
            .field("triangle", &self.triangle)
            .field("noise", &self.noise)
            .field("dmc", &self.dmc)
            .finish()
    }
}

pub(crate) static PULSE_TABLE: Lazy<[f32; 31]> = Lazy::new(|| {
    let mut pulse_table = [0.0; 31];
    for (i, val) in pulse_table.iter_mut().enumerate().skip(1) {
        *val = 95.52 / (8_128.0 / (i as f32) + 100.0);
    }
    pulse_table
});
static TND_TABLE: Lazy<[f32; 203]> = Lazy::new(|| {
    let mut tnd_table = [0.0; 203];
    for (i, val) in tnd_table.iter_mut().enumerate().skip(1) {
        *val = 163.67 / (24_329.0 / (i as f32) + 100.0);
    }
    tnd_table
});

#[cfg(test)]
impl Apu {
    pub(crate) const fn cycle(&self) -> usize {
        self.cycle
    }
}
