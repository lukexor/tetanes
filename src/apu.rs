//! Audio Processing Unit
//!
//! [https://wiki.nesdev.com/w/index.php/APU]()

use crate::{
    common::{Clocked, LogLevel, Loggable, Powered},
    cpu::CPU_CLOCK_RATE,
    filter::{Filter, HiPassFilter, LoPassFilter},
    mapper::MapperRef,
    memory::Memory,
    serialization::Savable,
    NesResult,
};
use std::{
    fmt,
    io::{Read, Write},
};

pub struct Divider {
    pub counter: f32,
    pub period: f32,
}

impl Divider {
    fn new(period: f32) -> Self {
        Self {
            counter: period,
            period,
        }
    }

    fn reset(&mut self) {
        self.counter = self.period;
    }
}

impl Clocked for Divider {
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

pub struct Sequencer {
    pub step: usize,
    pub length: usize,
}

impl Sequencer {
    fn new(length: usize) -> Self {
        Self { step: 1, length }
    }
}

impl Clocked for Sequencer {
    fn clock(&mut self) -> usize {
        let clock = self.step;
        self.step += 1;
        if self.step > self.length {
            self.step = 1;
        }
        clock as usize
    }
}

pub struct FrameSequencer {
    pub divider: Divider,
    pub sequencer: Sequencer,
    pub mode: FcMode,
}

impl FrameSequencer {
    fn new() -> Self {
        Self {
            divider: Divider::new(7457.5),
            sequencer: Sequencer::new(4),
            mode: FcMode::Step4,
        }
    }

    // On write to $4017
    fn reset(&mut self, val: u8) {
        // Reset & Configure divider/sequencer
        self.divider.reset();
        self.sequencer = if val & 0x80 == 0x00 {
            self.mode = FcMode::Step4;
            Sequencer::new(4)
        } else {
            self.mode = FcMode::Step5;
            let mut sequencer = Sequencer::new(5);
            let _ = sequencer.clock(); // Clock immediately
            sequencer
        };
    }
}

impl Clocked for FrameSequencer {
    fn clock(&mut self) -> usize {
        // Clocks at 240Hz
        // or 21_477_270 Hz / 89_490
        if self.divider.clock() == 1 {
            self.sequencer.clock()
        } else {
            0
        }
    }
}

pub const SAMPLE_RATE: f32 = 96_000.0; // in Hz
pub const SAMPLE_BUFFER_SIZE: usize = 4096;

/// Audio Processing Unit
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
    pub dmc: Dmc,
    log_level: LogLevel,
    filters: [Box<dyn Filter>; 3],
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
            log_level: LogLevel::Off,
            filters: [
                Box::new(HiPassFilter::new(90.0, SAMPLE_RATE)),
                Box::new(HiPassFilter::new(440.0, SAMPLE_RATE)),
                Box::new(LoPassFilter::new(14_000.0, SAMPLE_RATE)),
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

    pub fn load_mapper(&mut self, mapper: MapperRef) {
        self.dmc.mapper = Some(mapper);
    }

    pub fn samples(&mut self) -> &[f32] {
        &self.samples
    }

    pub fn clear_samples(&mut self) {
        self.samples.clear();
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.clock_rate = CPU_CLOCK_RATE * speed;
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
        self.triangle.enabled = false;
        let pulse1 = self.pulse1.output();
        let pulse2 = self.pulse2.output();
        let triangle = self.triangle.output();
        let noise = self.noise.output();
        let dmc = self.dmc.output();

        let pulse_out = self.pulse_table[(pulse1 + pulse2) as usize];
        let tnd_out = self.tnd_table[(3.0 * triangle + 2.0 * noise + dmc) as usize];
        pulse_out + tnd_out
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
        self.frame_sequencer.reset(val);
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
            for i in 0..self.filters.len() {
                let filter = &mut self.filters[i];
                sample = filter.process(sample);
            }
            self.samples.push(sample);
        }
        self.cycle += 1;
        1
    }
}

impl Loggable for Apu {
    fn set_log_level(&mut self, level: LogLevel) {
        self.log_level = level;
    }
    fn log_level(&mut self) -> LogLevel {
        self.log_level
    }
}

impl Memory for Apu {
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
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.irq_pending.save(fh)?;
        self.irq_enabled.save(fh)?;
        self.open_bus.save(fh)?;
        self.cycle.save(fh)?;
        self.pulse1.save(fh)?;
        self.pulse2.save(fh)?;
        self.triangle.save(fh)?;
        self.noise.save(fh)?;
        self.dmc.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.irq_pending.load(fh)?;
        self.irq_enabled.load(fh)?;
        self.open_bus.load(fh)?;
        self.cycle.load(fh)?;
        self.pulse1.load(fh)?;
        self.pulse2.load(fh)?;
        self.triangle.load(fh)?;
        self.noise.load(fh)?;
        self.dmc.load(fh)
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Apu {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "APU {{ cyc: {} }}", self.cycle)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum FcMode {
    Step4,
    Step5,
}

impl Savable for FcMode {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => FcMode::Step4,
            1 => FcMode::Step5,
            _ => panic!("invalid FcMode value"),
        };
        Ok(())
    }
}

struct Pulse {
    enabled: bool,
    duty_cycle: u8,        // Select row in DUTY_TABLE
    duty_counter: u8,      // Select column in DUTY_TABLE
    freq_timer: u16,       // timer freq_counter reload value
    freq_counter: u16,     // Current frequency timer value
    channel: PulseChannel, // One or Two
    length: LengthCounter,
    envelope: Envelope,
    sweep: Sweep,
}

impl Pulse {
    const DUTY_TABLE: [[u8; 8]; 4] = [
        [0, 1, 0, 0, 0, 0, 0, 0],
        [0, 1, 1, 0, 0, 0, 0, 0],
        [0, 1, 1, 1, 1, 0, 0, 0],
        [1, 0, 0, 1, 1, 1, 1, 1],
    ];

    fn new(channel: PulseChannel) -> Self {
        Self {
            enabled: false,
            duty_cycle: 0u8,
            duty_counter: 0u8,
            freq_timer: 0u16,
            freq_counter: 0u16,
            channel,
            length: LengthCounter::new(),
            envelope: Envelope::new(),
            sweep: Sweep {
                enabled: false,
                reload: false,
                negate: false,
                timer: 0u8,
                counter: 0u8,
                shift: 0u8,
            },
        }
    }

    fn reset(&mut self) {
        *self = Self::new(self.channel);
    }

    fn clock(&mut self) {
        if self.freq_counter > 0 {
            self.freq_counter -= 1;
        } else {
            self.freq_counter = self.freq_timer;
            self.duty_counter = (self.duty_counter + 1) % 8;
        }
    }
    fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }
    fn clock_half_frame(&mut self) {
        let sweep_forcing_silence = self.sweep_forcing_silence();
        let mut swp = &mut self.sweep;
        if swp.reload {
            swp.counter = swp.timer;
            swp.reload = false;
        } else if swp.counter > 0 {
            swp.counter -= 1;
        } else {
            swp.counter = swp.timer;
            if swp.enabled && !sweep_forcing_silence {
                let delta = self.freq_timer >> swp.shift;
                if swp.negate {
                    self.freq_timer -= delta + 1;
                    if self.channel == PulseChannel::One {
                        self.freq_timer += 1;
                    }
                } else {
                    self.freq_timer += delta
                }
            }
        }

        self.length.clock();
    }

    fn sweep_forcing_silence(&self) -> bool {
        let next_freq = self.freq_timer + (self.freq_timer >> self.sweep.shift);
        self.freq_timer < 8 || (!self.sweep.negate && next_freq >= 0x800)
    }

    fn output(&self) -> f32 {
        if Self::DUTY_TABLE[self.duty_cycle as usize][self.duty_counter as usize] != 0
            && self.length.counter != 0
            && !self.sweep_forcing_silence()
        {
            if self.envelope.enabled {
                f32::from(self.envelope.volume)
            } else {
                f32::from(self.envelope.constant_volume)
            }
        } else {
            0f32
        }
    }

    // $4000 Pulse control
    fn write_control(&mut self, val: u8) {
        self.duty_cycle = (val >> 6) & 0x03; // D7..D6
        self.length.write_control(val);
        self.envelope.write_control(val);
    }
    // $4001 Pulse sweep
    fn write_sweep(&mut self, val: u8) {
        self.sweep.timer = (val >> 4) & 0x07; // D6..D4
        self.sweep.negate = (val >> 3) & 1 == 1; // D3
        self.sweep.shift = val & 0x07; // D2..D0
        self.sweep.enabled = ((val >> 7) & 1 == 1) && (self.sweep.shift != 0); // D7
        self.sweep.reload = true;
    }
    // $4002 Pulse timer lo
    fn write_timer_lo(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0xFF00) | u16::from(val); // D7..D0
    }
    // $4003 Pulse timer hi
    fn write_timer_hi(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0x00FF) | u16::from(val & 0x07) << 8; // D2..D0
        self.freq_counter = self.freq_timer;
        self.duty_counter = 0;
        self.envelope.reset = true;
        if self.enabled {
            self.length.load_value(val);
        }
    }
}

impl Savable for Pulse {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.duty_cycle.save(fh)?;
        self.duty_counter.save(fh)?;
        self.freq_timer.save(fh)?;
        self.freq_counter.save(fh)?;
        self.channel.save(fh)?;
        self.length.save(fh)?;
        self.envelope.save(fh)?;
        self.sweep.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.duty_cycle.load(fh)?;
        self.duty_counter.load(fh)?;
        self.freq_timer.load(fh)?;
        self.freq_counter.load(fh)?;
        self.channel.load(fh)?;
        self.length.load(fh)?;
        self.envelope.load(fh)?;
        self.sweep.load(fh)
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
enum PulseChannel {
    One,
    Two,
}

impl Savable for PulseChannel {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => PulseChannel::One,
            1 => PulseChannel::Two,
            _ => panic!("invalid PulseChannel value"),
        };
        Ok(())
    }
}

struct Triangle {
    enabled: bool,
    ultrasonic: bool,
    step: u8,
    freq_timer: u16,
    freq_counter: u16,
    length: LengthCounter,
    linear: LinearCounter,
}

impl Triangle {
    fn new() -> Self {
        Self {
            enabled: false,
            ultrasonic: false,
            step: 0u8,
            freq_timer: 0u16,
            freq_counter: 0u16,
            length: LengthCounter::new(),
            linear: LinearCounter::new(),
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn clock(&mut self) {
        self.ultrasonic = false;
        if self.length.counter > 0 && self.freq_timer < 2 && self.freq_counter == 0 {
            self.ultrasonic = true;
        }

        let should_clock =
            !(self.length.counter == 0 || self.linear.counter == 0 || self.ultrasonic);
        if should_clock {
            if self.freq_counter > 0 {
                self.freq_counter -= 1;
            } else {
                self.freq_counter = self.freq_timer;
                self.step = (self.step + 1) & 0x1F;
            }
        }
    }

    fn clock_quarter_frame(&mut self) {
        if self.linear.reload {
            self.linear.counter = self.linear.load;
        } else if self.linear.counter > 0 {
            self.linear.counter -= 1;
        }
        if !self.linear.control {
            self.linear.reload = false;
        }
    }

    fn clock_half_frame(&mut self) {
        self.length.clock();
    }

    fn output(&self) -> f32 {
        if self.ultrasonic {
            7.5
        } else if self.step & 0x10 == 0x10 {
            f32::from(self.step ^ 0x1F)
        } else {
            f32::from(self.step)
        }
    }

    fn write_linear_counter(&mut self, val: u8) {
        self.linear.control = (val >> 7) & 1 == 1; // D7
        self.length.enabled = (val >> 7) & 1 == 0; // !D7
        self.linear.load_value(val);
    }

    fn write_timer_lo(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0xFF00) | u16::from(val); // D7..D0
    }

    fn write_timer_hi(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0x00FF) | u16::from(val & 0x07) << 8; // D2..D0
        self.freq_counter = self.freq_timer;
        self.linear.reload = true;
        if self.enabled {
            self.length.load_value(val);
        }
    }
}

impl Savable for Triangle {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.ultrasonic.save(fh)?;
        self.step.save(fh)?;
        self.freq_timer.save(fh)?;
        self.freq_counter.save(fh)?;
        self.length.save(fh)?;
        self.linear.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.ultrasonic.load(fh)?;
        self.step.load(fh)?;
        self.freq_timer.load(fh)?;
        self.freq_counter.load(fh)?;
        self.length.load(fh)?;
        self.linear.load(fh)
    }
}

struct Noise {
    enabled: bool,
    freq_timer: u16,       // timer freq_counter reload value
    freq_counter: u16,     // Current frequency timer value
    shift: u16,            // Must never be 0
    shift_mode: ShiftMode, // Zero (XOR bits 0 and 1) or One (XOR bits 0 and 6)
    length: LengthCounter,
    envelope: Envelope,
}

impl Noise {
    const FREQ_TABLE: [u16; 16] = [
        4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
    ];
    const SHIFT_BIT_15_MASK: u16 = !0x8000;

    fn new() -> Self {
        Self {
            enabled: false,
            freq_timer: 0u16,
            freq_counter: 0u16,
            shift: 1u16, // Must never be 0
            shift_mode: ShiftMode::Zero,
            length: LengthCounter::new(),
            envelope: Envelope::new(),
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn clock(&mut self) {
        if self.freq_counter > 0 {
            self.freq_counter -= 1;
        } else {
            self.freq_counter = self.freq_timer;
            let shift_amount = if self.shift_mode == ShiftMode::One {
                6
            } else {
                1
            };
            let bit1 = self.shift & 1; // Bit 0
            let bit2 = (self.shift >> shift_amount) & 1; // Bit 1 or 6 from above
            self.shift = (self.shift & Self::SHIFT_BIT_15_MASK) | ((bit1 ^ bit2) << 14);
            self.shift >>= 1;
        }
    }

    fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    fn clock_half_frame(&mut self) {
        self.length.clock();
    }

    fn output(&self) -> f32 {
        if self.shift & 1 == 0 && self.length.counter != 0 {
            if self.envelope.enabled {
                f32::from(self.envelope.volume)
            } else {
                f32::from(self.envelope.constant_volume)
            }
        } else {
            0f32
        }
    }

    fn write_control(&mut self, val: u8) {
        self.length.write_control(val);
        self.envelope.write_control(val);
    }

    // $400E Noise timer
    fn write_timer(&mut self, val: u8) {
        self.freq_timer = Self::FREQ_TABLE[(val & 0x0F) as usize];
        self.shift_mode = if (val >> 7) & 1 == 1 {
            ShiftMode::One
        } else {
            ShiftMode::Zero
        };
    }

    fn write_length(&mut self, val: u8) {
        if self.enabled {
            self.length.load_value(val);
        }
        self.envelope.reset = true;
    }
}

impl Savable for Noise {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.freq_timer.save(fh)?;
        self.freq_counter.save(fh)?;
        self.shift.save(fh)?;
        self.shift_mode.save(fh)?;
        self.length.save(fh)?;
        self.envelope.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.freq_timer.load(fh)?;
        self.freq_counter.load(fh)?;
        self.shift.load(fh)?;
        self.shift_mode.load(fh)?;
        self.length.load(fh)?;
        self.envelope.load(fh)
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
enum ShiftMode {
    Zero,
    One,
}

impl Savable for ShiftMode {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => ShiftMode::Zero,
            1 => ShiftMode::One,
            _ => panic!("invalid ShiftMode value"),
        };
        Ok(())
    }
}

pub struct Dmc {
    mapper: Option<MapperRef>,
    irq_enabled: bool,
    pub irq_pending: bool,
    loops: bool,
    freq_timer: u16,
    freq_counter: u16,
    addr: u16,
    addr_load: u16,
    length: u8,
    length_load: u8,
    sample_buffer: u8,
    sample_buffer_empty: bool,
    output: u8,
    output_bits: u8,
    output_shift: u8,
    output_silent: bool,
    log_level: LogLevel,
}

impl Dmc {
    // NTSC
    const NTSC_FREQ_TABLE: [u16; 16] = [
        0x1AC, 0x17C, 0x154, 0x140, 0x11E, 0x0FE, 0x0E2, 0x0D6, 0x0BE, 0x0A0, 0x08E, 0x080, 0x06A,
        0x054, 0x048, 0x036,
    ];
    fn new() -> Self {
        Self {
            mapper: None,
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
            log_level: LogLevel::Off,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn clock(&mut self) {
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
                if !self.sample_buffer_empty {
                    self.output_shift = self.sample_buffer;
                    self.sample_buffer_empty = true;
                    self.output_silent = false;
                } else {
                    self.output_silent = true;
                }
            }
        }

        if let Some(mapper) = &self.mapper {
            if self.length > 0 && self.sample_buffer_empty {
                self.sample_buffer = mapper.borrow_mut().read(self.addr);
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
        }
    }

    fn output(&self) -> f32 {
        f32::from(self.output)
    }

    // $4010 DMC timer
    fn write_timer(&mut self, val: u8) {
        self.irq_enabled = (val >> 7) & 1 == 1;
        self.loops = (val >> 6) & 1 == 1;
        self.freq_timer = Self::NTSC_FREQ_TABLE[(val & 0x0F) as usize];
        if !self.irq_enabled {
            self.irq_pending = false;
        }
    }

    // $4011 DMC output
    fn write_output(&mut self, val: u8) {
        self.output = val >> 1;
    }

    // $4012 DMC addr load
    fn write_addr_load(&mut self, val: u8) {
        self.addr_load = 0xC000 | (u16::from(val) << 6);
    }

    // $4013 DMC length
    fn write_length(&mut self, val: u8) {
        self.length_load = (val << 4) + 1;
    }
}

impl Loggable for Dmc {
    fn set_log_level(&mut self, level: LogLevel) {
        self.log_level = level;
    }
    fn log_level(&mut self) -> LogLevel {
        self.log_level
    }
}

impl Savable for Dmc {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
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
        self.output_silent.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
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
        self.output_silent.load(fh)
    }
}

struct LengthCounter {
    enabled: bool,
    counter: u8, // Entry into LENGTH_TABLE
}

impl LengthCounter {
    const LENGTH_TABLE: [u8; 32] = [
        10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96,
        22, 192, 24, 72, 26, 16, 28, 32, 30,
    ];

    fn new() -> Self {
        Self {
            enabled: false,
            counter: 0u8,
        }
    }

    fn clock(&mut self) {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
        }
    }

    fn load_value(&mut self, val: u8) {
        self.counter = Self::LENGTH_TABLE[(val >> 3) as usize]; // D7..D3
    }

    fn write_control(&mut self, val: u8) {
        self.enabled = (val >> 5) & 1 == 0; // !D5
    }
}

impl Savable for LengthCounter {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.counter.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.counter.load(fh)
    }
}

struct LinearCounter {
    reload: bool,
    control: bool,
    load: u8,
    counter: u8,
}

impl LinearCounter {
    fn new() -> Self {
        Self {
            reload: false,
            control: false,
            load: 0u8,
            counter: 0u8,
        }
    }

    fn load_value(&mut self, val: u8) {
        self.load = val >> 1; // D6..D0
    }
}

impl Savable for LinearCounter {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.reload.save(fh)?;
        self.control.save(fh)?;
        self.load.save(fh)?;
        self.counter.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.reload.load(fh)?;
        self.control.load(fh)?;
        self.load.load(fh)?;
        self.counter.load(fh)
    }
}

struct Envelope {
    enabled: bool,
    loops: bool,
    reset: bool,
    volume: u8,
    constant_volume: u8,
    counter: u8,
}

impl Envelope {
    fn new() -> Self {
        Self {
            enabled: false,
            loops: false,
            reset: false,
            volume: 0u8,
            constant_volume: 0u8,
            counter: 0u8,
        }
    }

    fn clock(&mut self) {
        if self.reset {
            self.reset = false;
            self.volume = 0x0F;
            self.counter = self.constant_volume;
        } else if self.counter > 0 {
            self.counter -= 1;
        } else {
            self.counter = self.constant_volume;
            if self.volume > 0 {
                self.volume -= 1;
            } else if self.loops {
                self.volume = 0x0F;
            }
        }
    }

    // $4000/$4004/$400C Envelope control
    fn write_control(&mut self, val: u8) {
        self.loops = (val >> 5) & 1 == 1; // D5
        self.enabled = (val >> 4) & 1 == 0; // !D4
        self.constant_volume = val & 0x0F; // D3..D0
    }
}

impl Savable for Envelope {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.loops.save(fh)?;
        self.reset.save(fh)?;
        self.volume.save(fh)?;
        self.constant_volume.save(fh)?;
        self.counter.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.loops.load(fh)?;
        self.reset.load(fh)?;
        self.volume.load(fh)?;
        self.constant_volume.load(fh)?;
        self.counter.load(fh)
    }
}

struct Sweep {
    enabled: bool,
    reload: bool,
    negate: bool, // Treats PulseChannel 1 differently than PulseChannel 2
    timer: u8,    // counter reload value
    counter: u8,  // current timer value
    shift: u8,
}

impl Savable for Sweep {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.reload.save(fh)?;
        self.negate.save(fh)?;
        self.timer.save(fh)?;
        self.counter.save(fh)?;
        self.shift.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.reload.load(fh)?;
        self.negate.load(fh)?;
        self.timer.load(fh)?;
        self.counter.load(fh)?;
        self.shift.load(fh)
    }
}

#[cfg(test)]
mod tests {}
