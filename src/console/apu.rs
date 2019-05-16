use crate::console::Memory;
use std::fmt;

const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
const SQUARE_DUTY_TABLE: [u8; 4] = [0b01000000, 0b01100000, 0b01111000, 0b10011111];
const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];
const DMC_TABLE: [u8; 16] = [
    214, 190, 170, 160, 143, 127, 113, 107, 95, 80, 71, 64, 53, 42, 36, 27,
];

// Audio Processing Unit
pub struct Apu {
    cycles: u64,
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
    enabled: bool,
    reload: bool,
    reload_value: u8,
    counter: u8,
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
            sample_rate: 1024.0 / 30000.0,
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

    // Clocking

    pub fn step(&mut self) {
        if self.cycles % 2 == 0 {
            self.frame_counter.step();
            self.square1.step();
            self.square2.step();
            self.noise.step();
            self.dmc.step();
        }
        self.triangle.step();

        if self.frame_counter.step_half_frame() {
            self.square1.step_half_frame();
            self.square2.step_half_frame();
            self.triangle.step_half_frame();
            self.noise.step_half_frame();
            self.dmc.step_half_frame();
        }

        if self.frame_counter.step_quarter_frame() {
            self.square1.step_quarter_frame();
            self.square2.step_quarter_frame();
            self.triangle.step_quarter_frame();
            self.noise.step_quarter_frame();
            self.dmc.step_half_frame();
        }

        if self.sample_timer >= 1.0 {
            let sample = self.sample();
            self.samples.push(sample);
            self.sample_timer %= 1.0;
        }
        self.sample_timer += self.sample_rate;
        self.cycles += 1;
    }

    // Getters

    fn sample(&self) -> f32 {
        let square1 = self.square1.sample();
        let square2 = self.square2.sample();
        let triangle = self.triangle.sample();
        let noise = self.noise.sample();
        let dmc = self.dmc.sample();

        let square_out = 0.00752 * (square1 + square2);
        let tnd_out = 0.00851 * triangle + 0.00494 * noise + 0.00335 * dmc;
        (square_out + tnd_out) as f32
    }
    fn read_status(&self) -> u8 {
        let status = self.square1.enabled() as u8
            | (self.square2.enabled() as u8) << 1
            | (self.triangle.enabled() as u8) << 2
            | (self.noise.enabled() as u8) << 3
            | (self.dmc.enabled() as u8) << 4;
        status
    }

    // Memory accesses

    fn write_status(&mut self, val: u8) {
        if val & 0x01 == 0x01 {
            self.square1.enable();
        } else if val & 0x02 == 0x02 {
            self.square2.enable();
        } else if val & 0x04 == 0x04 {
            self.triangle.enable();
        } else if val & 0x08 == 0x08 {
            self.noise.enable();
        } else if val & 0x10 == 0x10 {
            self.dmc.enable();
        }
    }
    fn write_frame_counter(&mut self, val: u8) {
        self.frame_counter.write_control(val);
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
        // eprintln!("{:?}", self);
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

    // Clocking

    fn step(&mut self) {
        if self.timer == 0 {
            self.sequencer_step = self.sequencer_step.wrapping_sub(1) & 0x7;
            self.timer = self.timer_period;
        } else {
            self.timer -= 1;
        }
    }
    fn step_half_frame(&mut self) {
        self.timer_period = self.sweep.period;
        self.sweep.period = self.timer_period;
        self.sweep.step();
        self.length_counter.step();
    }
    fn step_quarter_frame(&mut self) {
        self.envelope.step();
    }

    // Getters

    fn sample(&self) -> f64 {
        if !self.silenced() {
            self.sequencer() * self.envelope.sample()
        } else {
            0f64
        }
    }
    fn sequencer(&self) -> f64 {
        let duty = SQUARE_DUTY_TABLE[self.duty_cycle as usize];
        let val = (duty >> (7 - self.sequencer_step % 8)) & 1;
        val as f64
    }

    fn silenced(&self) -> bool {
        self.sweep.silenced() || self.length_counter.silenced()
    }
    fn enabled(&self) -> bool {
        self.length_counter.enabled()
    }

    // Setters

    fn enable(&mut self) {
        self.length_counter.enable();
    }
    fn disable(&mut self) {
        self.length_counter.disable();
    }

    // Memory accesses

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
        self.sweep.period = self.timer_period;
    }
    fn write_timer_hi(&mut self, val: u8) {
        self.timer_period = (self.timer_period & 0x00FF) | (u16::from(val & 0x07) << 8);
        self.length_counter.set_load(val >> 3);
        self.sequencer_step = 0u8;
        self.envelope.reset();
        self.sweep.period = self.timer_period;
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

    // Clocking

    fn step(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            if !self.silenced() {
                self.sequencer_step = (self.sequencer_step + 1) & 0x1F;
            }
        } else {
            self.timer -= 1;
        }
    }
    fn step_half_frame(&mut self) {
        self.length_counter.step()
    }
    fn step_quarter_frame(&mut self) {
        self.linear_counter.step()
    }

    // Getters

    fn sample(&self) -> f64 {
        let step = self.sequencer_step;
        let sample = if step < 16 { 15 - step } else { step - 16 };
        sample as f64
    }
    fn silenced(&self) -> bool {
        self.length_counter.silenced() || self.linear_counter.silenced()
    }
    fn enabled(&self) -> bool {
        self.length_counter.enabled()
    }

    // Setters

    fn enable(&mut self) {
        self.length_counter.enable();
        self.linear_counter.enable();
    }
    fn disable(&mut self) {
        self.length_counter.disable();
        self.linear_counter.disable();
    }

    // Memory accesses

    fn write_linear_counter(&mut self, val: u8) {
        let flag = val & 0x80 > 0;
        self.length_counter.halt = flag;
        self.linear_counter.reload = flag;
        self.linear_counter.reload_value = val & 0x7F;
    }
    fn write_timer_lo(&mut self, val: u8) {
        self.timer_period = (self.timer_period & 0xFF00) | u16::from(val);
    }
    fn write_timer_hi(&mut self, val: u8) {
        self.timer_period = (self.timer_period & 0xFF00) | (u16::from(val & 0x07) << 8);
        self.length_counter.set_load(val >> 3);
        self.linear_counter.reload = true;
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

    // Clocking

    fn step(&mut self) {
        if self.timer == 0 {
            self.timer = self.period;
            self.feedback = self.next_feedback();
        } else {
            self.timer -= 1;
        }
    }
    fn step_half_frame(&mut self) {
        self.length_counter.step();
    }
    fn step_quarter_frame(&mut self) {
        self.envelope.step();
    }

    // Getters

    fn sample(&self) -> f64 {
        if self.feedback & 0x01 > 0 && !self.silenced() {
            self.envelope.sample()
        } else {
            0f64
        }
    }
    fn silenced(&self) -> bool {
        self.length_counter.silenced()
    }
    fn enabled(&self) -> bool {
        self.length_counter.enabled()
    }
    fn next_feedback(&self) -> u16 {
        let mut feedback = self.feedback;
        let newbit = if self.mode {
            (feedback & 0x01) ^ ((feedback >> 6) & 0x01) // XOR bit 0 with bit 6
        } else {
            (feedback & 0x01) ^ ((feedback >> 1) & 0x01) // XOR bit 0 with bit 1
        };
        feedback |= if newbit > 0 { 1 << 14 } else { 0u16 };
        feedback
    }

    // Setters

    fn enable(&mut self) {
        self.length_counter.enabled();
    }
    fn disable(&mut self) {
        self.length_counter.disable();
    }

    // Memory accesses

    fn write_volume(&mut self, val: u8) {
        self.length_counter.halt = val & 0x20 > 0;
        self.envelope.write_control(val);
    }
    fn write_period(&mut self, val: u8) {
        self.mode = val & 0x08 > 0;
        self.period = NOISE_PERIOD_TABLE[(val & 0x0F) as usize];
    }
    fn write_length(&mut self, val: u8) {
        self.length_counter.set_load(val >> 3);
        self.envelope.reset();
    }
}

impl DMC {
    fn new() -> Self {
        Self {}
    }

    // Clocking

    fn step(&mut self) {
        // TODO
    }
    fn step_half_frame(&mut self) {
        // TODO
    }

    // Getters

    fn sample(&self) -> f64 {
        0f64
    }
    fn enabled(&self) -> bool {
        // TODO
        false
    }

    // Setters

    fn enable(&mut self) {
        // TODO
    }
    fn disable(&mut self) {
        // TODO
    }

    // Memory accesses

    fn write_frequency(&mut self, val: u8) {
        // TODO
    }
    fn write_raw(&mut self, val: u8) {
        // TODO
    }
    fn write_waveform(&mut self, val: u8) {
        // TODO
    }
    fn write_length(&mut self, val: u8) {
        // TODO
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

    // Clocking

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
    fn step_quarter_frame(&self) -> bool {
        match (&self.mode, self.step) {
            (_, 3728) | (_, 7456) | (_, 11185) | (Step4, 14914) | (Step5, 18640) => true,
            _ => false,
        }
    }
    fn step_half_frame(&self) -> bool {
        match (&self.mode, self.step) {
            (_, 7456) | (Step4, 14914) | (Step5, 18640) => true,
            _ => false,
        }
    }

    // Memory accesses

    fn write_control(&mut self, val: u8) {
        self.mode = if val & 0x80 > 0 { Step5 } else { Step4 };
        self.interrupt_disable = val & 0x40 > 0;
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

    // Clocking

    fn step(&mut self) {
        if self.counter > 0 && !self.halt {
            self.counter -= 1;
        }
    }

    // Getters

    fn silenced(&self) -> bool {
        self.counter == 0 || !self.enabled
    }
    fn enabled(&self) -> bool {
        self.enabled
    }

    // Setters

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

    // Clocking

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

    // Getters

    fn silenced(&self) -> bool {
        self.counter == 0
    }
    fn enabled(&self) -> bool {
        self.enabled
    }

    // Setters

    fn enable(&mut self) {
        self.enabled = true;
    }
    fn disable(&mut self) {
        self.enabled = false;
    }
}

impl Sweep {
    pub fn new(channel: SqChan) -> Self {
        Self {
            enabled: false,
            reload: false,
            negate: false,
            divider: 0u8,
            divider_period: 0u8,
            shift_count: 0u8,
            timer: 0u16,
            period: 0u16,
            channel,
        }
    }

    // Clocking

    fn step(&mut self) {
        if self.divider == 0 && self.enabled && !self.silenced() && self.shift_count != 0 {
            self.period = self.target_period();
        }
        if self.divider == 0 || self.reload {
            self.divider = self.divider_period;
            self.reload = false;
        } else {
            self.divider -= 1;
        }
    }

    // Getters

    fn silenced(&self) -> bool {
        self.period < 8 || self.target_period() > 0x7FF
    }
    fn target_period(&self) -> u16 {
        let mut change = self.timer >> self.shift_count;
        if self.negate {
            change = self.negate(change);
        }
        self.period.wrapping_add(change)
    }

    // Memory accesses

    fn write_control(&mut self, val: u8) {
        self.shift_count = val & 0x07;
        self.negate = val & 0x04 > 0;
        self.divider_period = (val >> 4) & 0x7;
        self.enabled = val & 0x80 > 0;
    }

    // Helpers

    fn negate(&self, val: u16) -> u16 {
        match self.channel {
            SqChan::One => (-(val as i16)) as u16,
            SqChan::Two => (-(val as i16) - 1) as u16,
        }
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

    // Clocking

    fn step(&mut self) {
        if self.start {
            self.start = false;
            self.volume = 15;
            self.divider = self.period;
        } else if self.divider > 0 {
            self.divider -= 1;
        } else {
            if self.volume > 0 {
                self.volume -= 1;
            } else if self.looping {
                self.volume = 15;
            }
            self.divider = self.period;
        }
    }

    // Getters

    fn sample(&self) -> f64 {
        let sample = if self.constant {
            self.period
        } else {
            self.volume
        };
        sample as f64
    }

    // Setters

    fn reset(&mut self) {
        self.start = true;
    }

    // Memory accesses

    fn write_control(&mut self, val: u8) {
        self.period = val & 0xf;
        self.volume = val & 0xf;
        self.constant = val & 0x08 > 0;
        self.looping = val & 0x10 > 0;
    }
}

impl fmt::Debug for Apu {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "APU {{ cyc: {}, sample_timer: {} }}",
            self.cycles, self.sample_timer,
        )
    }
}

#[cfg(test)]
mod tests {}
