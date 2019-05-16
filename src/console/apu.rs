use crate::console::{Cycles, Memory};
use std::fmt;

const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];
const TRIANGLE_TABLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];
const NOISE_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];
const DMC_TABLE: [u8; 16] = [
    214, 190, 170, 160, 143, 127, 113, 107, 95, 80, 71, 64, 53, 42, 36, 27,
];

// Audio Processing Unit
pub struct Apu {
    cycles: Cycles,
    sample_rate: f64,
    sample_timer: f64,
    frame_counter: FrameCounter,
    square1: Square,
    square2: Square,
    triangle: Triangle,
    noise: Noise,
    dmc: DMC,
    pub samples: Vec<f32>,
}

enum SqChan {
    One,
    Two,
}
struct Square {
    sequencer_step: u8,
    duty_cycle: u8,
    timer: u16,
    timer_period: u16,
    sweep: Sweep,
    envelope: Envelope,
    length_counter: LengthCounter,
}

struct Triangle {
    sequencer_step: u8,
    timer: u16,
    timer_period: u16,
    length_counter: LengthCounter,
    linear_counter: LinearCounter,
}

struct Noise {
    mode: bool,
    timer: u16,
    period: u16,
    feedback: u16,
    envelope: Envelope,
    length_counter: LengthCounter,
}

// TODO Delta Modulation Channel
struct DMC {}

// https://wiki.nesdev.com/w/index.php/APU_Frame_Counter
struct FrameCounter {
    interrupt_disable: bool,
    step: u16,
    mode: CounterMode,
}
enum CounterMode {
    Step4,
    Step5,
}
use CounterMode::*;

// https://wiki.nesdev.com/w/index.php/APU_Length_Counter
struct LengthCounter {
    enabled: bool,
    halt: bool,
    counter: u8,
}
// https://wiki.nesdev.com/w/index.php/APU_Triangle
struct LinearCounter {
    pub enabled: bool,
    pub reload: bool,
    pub reload_value: u8,
    pub counter: u8,
}
// https://wiki.nesdev.com/w/index.php/APU_Sweep
struct Sweep {
    enabled: bool,
    reload: bool,
    negate: bool,
    divider: u8,
    divider_period: u8,
    shift_count: u8,
    timer: u16,
    period: u16,
    channel: SqChan, // Channel 1 or 2 - 1 adds with one's complement; 2 adds with two's complement
}
struct Envelope {
    start: bool,
    looping: bool,
    constant: bool,
    divider: u8,
    period: u8,
    volume: u8,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            cycles: 0u64,
            sample_rate: 0f64,
            sample_timer: 0f64,
            frame_counter: FrameCounter::new(),
            square1: Square::new(SqChan::One),
            square2: Square::new(SqChan::Two),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: DMC::new(),
            samples: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        unimplemented!();
    }

    pub fn step(&mut self) {
        if self.cycles % 2 == 0 {
            self.frame_counter.step();
            self.square1.step();
            self.square2.step();
            self.noise.step();
        }
        self.triangle.step();

        if self.frame_counter.half_frame() {
            self.square1.step_half_frame();
            self.square2.step_half_frame();
            self.triangle.step_half_frame();
            self.noise.step_half_frame();
        }

        if self.frame_counter.quarter_frame() {
            self.square1.step_quarter_frame();
            self.square2.step_quarter_frame();
            self.triangle.step_quarter_frame();
            self.noise.step_quarter_frame();
        }

        if self.sample_timer >= 1.0 {
            let sample = self.sample();
            self.samples.push(sample);
            self.sample_timer %= 1.0;
        }
        self.sample_timer += self.sample_rate;
        self.cycles += 1;
    }

    fn sample(&self) -> f32 {
        unimplemented!()
    }
    fn read_status(&self) -> u8 {
        unimplemented!()
    }
    fn write_status(&mut self, val: u8) {
        unimplemented!()
    }
    fn write_frame_counter(&mut self, val: u8) {
        unimplemented!()
    }
}

impl Memory for Apu {
    fn readb(&mut self, addr: u16) -> u8 {
        if addr == 0x4015 {
            self.read_status()
        } else {
            eprintln!("unhandled Apu readb at address 0x{:04X}", addr);
            0u8
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            0x4000 => self.square1.write_volume(val),
            0x4001 => self.square1.write_sweep(val),
            0x4002 => self.square1.write_timer_lo(val),
            0x4003 => self.square1.write_timer_hi(val),
            0x4004 => self.square2.write_volume(val),
            0x4005 => self.square2.write_sweep(val),
            0x4006 => self.square2.write_timer_lo(val),
            0x4007 => self.square2.write_timer_hi(val),
            0x4008 => self.triangle.write_linear_counter(val),
            0x4009 => (), // Unused
            0x400A => self.triangle.write_timer_lo(val),
            0x400B => self.triangle.write_timer_hi(val),
            0x400C => self.noise.write_volume(val),
            0x400D => (), // Unused
            0x400E => self.noise.write_period(val),
            0x400F => self.noise.write_length(val),
            0x4010 => self.dmc.write_frequency(val),
            0x4011 => self.dmc.write_raw(val),
            0x4012 => self.dmc.write_waveform(val),
            0x4013 => self.dmc.write_length(val),
            0x4015 => self.write_status(val),
            0x4017 => self.write_frame_counter(val),
            _ => eprintln!("unhandled Apu writeb at address: 0x{:04X}", addr),
        }
    }
}

impl Square {
    fn new(channel: SqChan) -> Self {
        Self {
            sequencer_step: 0u8,
            duty_cycle: 0u8,
            timer: 0u16,
            timer_period: 0u16,
            sweep: Sweep::new(channel),
            envelope: Envelope::new(),
            length_counter: LengthCounter::new(),
        }
    }
    fn step(&mut self) {
        unimplemented!()
    }
    fn step_half_frame(&mut self) {
        unimplemented!()
    }
    fn step_quarter_frame(&mut self) {
        unimplemented!()
    }
    fn write_volume(&mut self, val: u8) {
        self.envelope.write_control(val);
        self.length_counter.halt = (val & 0x20 > 0);
        self.duty_cycle = val >> 6;
    }
    fn write_sweep(&mut self, val: u8) {
        self.sweep.write_control(val);
    }
    fn write_timer_lo(&mut self, val: u8) {
        self.timer_period = (self.timer_period & 0xFF00) | u16::from(val);
        let period = self.timer_period;
        self.sweep.period = period;
    }
    fn write_timer_hi(&mut self, val: u8) {
        unimplemented!()
    }
}

impl Triangle {
    fn new() -> Self {
        Self {
            sequencer_step: 0u8,
            timer: 0u16,
            timer_period: 0u16,
            length_counter: LengthCounter::new(),
            linear_counter: LinearCounter::new(),
        }
    }
    fn step(&mut self) {
        unimplemented!()
    }
    fn step_half_frame(&mut self) {
        unimplemented!()
    }
    fn step_quarter_frame(&mut self) {
        unimplemented!()
    }
    fn write_linear_counter(&mut self, val: u8) {
        unimplemented!()
    }
    fn write_timer_lo(&mut self, val: u8) {
        unimplemented!()
    }
    fn write_timer_hi(&mut self, val: u8) {
        unimplemented!()
    }
}

impl Noise {
    fn new() -> Self {
        Self {
            mode: false,
            timer: 0u16,
            period: 0u16,
            feedback: 1u16,
            envelope: Envelope::new(),
            length_counter: LengthCounter::new(),
        }
    }
    fn step(&mut self) {
        unimplemented!()
    }
    fn step_half_frame(&mut self) {
        unimplemented!()
    }
    fn step_quarter_frame(&mut self) {
        unimplemented!()
    }
    fn write_volume(&mut self, val: u8) {
        unimplemented!()
    }
    fn write_period(&mut self, val: u8) {
        unimplemented!()
    }
    fn write_length(&mut self, val: u8) {
        unimplemented!()
    }
}

impl DMC {
    fn new() -> Self {
        Self {}
    }
    fn write_frequency(&mut self, val: u8) {
        unimplemented!()
    }
    fn write_raw(&mut self, val: u8) {
        unimplemented!()
    }
    fn write_waveform(&mut self, val: u8) {
        unimplemented!()
    }
    fn write_length(&mut self, val: u8) {
        unimplemented!()
    }
}

impl FrameCounter {
    pub fn new() -> Self {
        Self {
            interrupt_disable: false,
            step: 0u16,
            mode: Step4,
        }
    }
    pub fn step(&mut self) {
        self.step += 1;
        let max_step = match self.mode {
            Step4 => 14915,
            Step5 => 18641,
        };
        if self.step >= max_step {
            self.step -= max_step;
        }
    }
    fn quarter_frame(&self) -> bool {
        match (&self.mode, self.step) {
            (_, 3728) | (_, 7456) | (_, 11185) | (Step4, 14914) | (Step5, 18640) => true,
            _ => false,
        }
    }
    fn half_frame(&self) -> bool {
        match (&self.mode, self.step) {
            (_, 7456) | (Step4, 14914) | (Step5, 18640) => true,
            _ => false,
        }
    }
}

impl LengthCounter {
    fn new() -> Self {
        Self {
            enabled: true,
            halt: false,
            counter: 0u8,
        }
    }
    fn step(&mut self) {
        if self.counter > 0 && !self.halt {
            self.counter -= 1;
        }
    }
    fn silenced(&self) -> bool {
        self.counter == 0 || !self.enabled
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn enable(&mut self) {
        self.enabled = true;
    }
    fn disable(&mut self) {
        self.counter = 0;
        self.enabled = false;
    }
    fn set_load(&mut self, val: u8) {
        if self.enabled {
            self.counter = LENGTH_TABLE[val as usize & 0x1F];
        }
    }
}

impl LinearCounter {
    pub fn new() -> Self {
        Self {
            enabled: false,
            reload: false,
            reload_value: 0u8,
            counter: 0u8,
        }
    }
    pub fn step(&mut self) {
        if self.reload {
            self.counter = self.reload_value;
        } else if self.counter > 0 {
            self.counter -= 1;
        }
        if self.enabled {
            self.reload = false;
        }
    }
    fn silenced(&self) -> bool {
        self.counter == 0
    }
}

impl Sweep {
    pub fn new(channel: SqChan) -> Self {
        Self {
            enabled: false,
            reload: false,
            negate: false,
            divider: 0,
            divider_period: 0,
            shift_count: 0,
            timer: 0,
            period: 0,
            channel,
        }
    }
    fn write_control(&mut self, val: u8) {
        self.shift_count = val & 0x07;
        self.negate = val & 0x04 > 0;
        self.divider_period = (val >> 4) & 0x7;
        self.enabled = val & 0x80 > 0;
    }
}

impl Envelope {
    pub fn new() -> Self {
        Self {
            start: false,
            looping: false,
            constant: false,
            divider: 0u8,
            period: 0u8,
            volume: 0u8,
        }
    }
    fn write_control(&mut self, val: u8) {
        self.period = val & 0xf;
        self.volume = val & 0xf;
        self.constant = val & 0x08 > 0;
        self.looping = val & 0x10 > 0;
    }
}

impl fmt::Debug for Apu {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "APU {{ }}",)
    }
}

#[cfg(test)]
mod tests {}
