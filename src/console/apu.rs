use crate::console::MASTER_CLOCK_RATE;
use crate::memory::Memory;
use std::fmt;

const FREQUENCY: f64 = 48_000.0; // 44,100 Hz
const APU_CLOCK_RATE: f64 = MASTER_CLOCK_RATE / 89_490.0;
const CYCLES_PER_SAMPLE: f64 = MASTER_CLOCK_RATE / 12.0 / FREQUENCY;

const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
// const NOISE_PERIOD_TABLE: [u16; 16] = [
//     4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
// ];
// const DMC_TABLE: [u8; 16] = [
//     214, 190, 170, 160, 143, 127, 113, 107, 95, 80, 71, 64, 53, 42, 36, 27,
// ];

// Audio Processing Unit
pub struct Apu {
    cycle: u64,                  // Current APU cycle = CPU cycle / 2
    irq_pending: bool, // Set by $4017 if irq_enabled is clear or set during step 4 of Step4 mode
    irq_enabled: bool, // Set by $4017 D6
    samples: Vec<f32>, // Buffer of samples
    frame_counter: FrameCounter, // Clocks length, linear, sweep, and envelope units
    pulse1: Pulse,
    pulse2: Pulse,
    // triangle: Triangle,
    // noise: Noise,
    // dmc: DMC,
    filter: Filter,
}

struct Filter {
    lp_out: f32,
    hpa_out: f32,
    hpa_prev: f32,
    hpb_out: f32,
    hpb_prev: f32,
}

impl Apu {
    pub fn new() -> Self {
        let mut apu = Self {
            cycle: 0u64,
            irq_pending: false,
            irq_enabled: false,
            samples: Vec::new(),
            frame_counter: FrameCounter {
                step: 1u8,
                counter: 0u16,
                mode: FCMode::Step4,
            },
            pulse1: Pulse::new(PulseChannel::One),
            pulse2: Pulse::new(PulseChannel::Two),
            // triangle: Triangle::new(),
            // noise: Noise::new(),
            // dmc: DMC::new(),
            filter: Filter {
                lp_out: 0f32,
                hpa_out: 0f32,
                hpa_prev: 0f32,
                hpb_out: 0f32,
                hpb_prev: 0f32,
            },
        };
        apu.frame_counter.counter = apu.next_frame_counter();
        apu
    }

    pub fn reset(&mut self) {
        self.cycle = 0;
        self.samples.clear();
        self.irq_pending = false;
        self.irq_enabled = false;
        // TODO clear channels too
    }

    pub fn clock(&mut self) {
        if self.cycle % 2 == 0 {
            self.pulse1.clock();
            self.pulse2.clock();
            // self.noise.step();
            // self.dmc.step();
        }
        // self.triangle.step();
        self.clock_frame_counter();

        if self.cycle % CYCLES_PER_SAMPLE as u64 == 0 {
            // self.pulse1.disable();
            // self.pulse2.disable();
            // self.triangle.disable();
            // self.noise.disable();
            let sample = self.sample();
            self.samples.push(sample);
        }
        self.cycle += 1;
    }

    fn apply_filters(&mut self, sample: f32) -> f32 {
        // low pass
        self.filter.lp_out = (sample - self.filter.lp_out) * 0.815686;

        // highpass 1
        self.filter.hpa_out =
            self.filter.hpa_out * 0.996039 + self.filter.lp_out - self.filter.hpa_prev;
        self.filter.hpa_prev = self.filter.hpa_out;

        // highpass 2
        self.filter.hpb_out =
            self.filter.hpb_out * 0.999835 + self.filter.hpa_out - self.filter.hpb_prev;
        self.filter.hpb_prev = self.filter.hpb_out;
        self.filter.hpb_out
    }

    // Counts CPU clocks and determines when to clock quarter/half frames
    // counter is in CPU clocks to avoid APU half-frames
    fn clock_frame_counter(&mut self) {
        use FCMode::*;

        if self.frame_counter.counter > 0 {
            self.frame_counter.counter -= 1;
        } else {
            match self.frame_counter.step {
                1 | 3 => self.clock_quarter_frame(),
                2 | 5 => {
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                }
                _ => (), // Noop
            }
            if self.irq_enabled
                && self.frame_counter.mode == Step4
                && self.frame_counter.step >= 4
                && self.frame_counter.step <= 6
            {
                self.irq_pending = true;
            }

            self.frame_counter.step += 1;
            let max_step = 6;
            if self.frame_counter.step > max_step {
                self.frame_counter.step = 1;
            }
            self.frame_counter.counter = self.next_frame_counter();
        }
    }

    fn clock_quarter_frame(&mut self) {
        self.pulse1.clock_quarter_frame();
        self.pulse2.clock_quarter_frame();
    }

    fn clock_half_frame(&mut self) {
        self.pulse1.clock_half_frame();
        self.pulse2.clock_half_frame();
    }

    fn next_frame_counter(&self) -> u16 {
        let fc = &self.frame_counter;
        match fc.mode {
            FCMode::Step4 => match fc.step {
                1 => 7_457, // 3728.5 APU cycles
                2 => 7_456, // 7456.5 APU cycles
                3 => 7_458, // 11185.5 APU cycles
                4 => 7_457, // 14914 APU cycles
                5 => 1,     // 14914.5 APU cycles
                6 => 1,     // 14915 APU cycles
                _ => 0,     // Noop
            },
            FCMode::Step5 => match fc.step {
                1 => 7_457, // 3728.5 APU cycles
                2 => 7_456, // 7456.5 APU cycles
                3 => 7_458, // 11185.5 APU cycles
                4 => 7_458, // 14914.5 APU cycles
                5 => 7_452, // 18640.5 APU cycles
                6 => 1,     // 18641 APU cycles
                _ => 0,     // Noop
            },
        }
    }

    pub fn samples(&mut self) -> &mut Vec<f32> {
        &mut self.samples
    }
    fn sample(&self) -> f32 {
        let pulse1 = self.pulse1.output();
        let pulse2 = self.pulse2.output();
        let triangle = 0; //self.triangle.sample();
        let noise = 0; //self.noise.sample();
        let dmc = 0; //self.dmc.sample();

        let pulse_out = 0.00752 * (pulse1 + pulse2) as f32;
        let tnd_out =
            0.00851 * (triangle as f32) + 0.00494 * (noise as f32) + 0.00335 * (dmc as f32);
        (pulse_out + tnd_out) as f32
    }
    fn read_status(&mut self) -> u8 {
        let status = self.pulse1.enabled as u8
            | (self.pulse2.enabled as u8) << 1
            | 0 << 2
            | 0 << 3
            | 0 << 4
            | 0 << 5
            | (self.irq_pending as u8) << 6
            | 0 << 7;
        self.irq_pending = false;
        status
    }
    fn write_status(&mut self, val: u8) {
        self.pulse1.enabled = val & 1 == 1;
        if !self.pulse1.enabled {
            self.pulse1.length_counter = 0;
        }
        self.pulse2.enabled = (val >> 1) & 1 == 1;
        if !self.pulse2.enabled {
            self.pulse2.length_counter = 0;
        }
    }

    fn write_frame_counter(&mut self, val: u8) {
        // D7
        self.frame_counter.mode = if (val >> 7) & 1 == 0 {
            FCMode::Step4
        } else {
            FCMode::Step5
        };
        self.frame_counter.step = 1;
        self.frame_counter.counter = self.next_frame_counter();
        if self.frame_counter.mode == FCMode::Step5 {
            self.clock_quarter_frame();
            self.clock_half_frame();
        }
        self.irq_enabled = (val >> 6) & 1 == 0; // D6
        if !self.irq_enabled {
            self.irq_pending = false;
        }
    }
}

impl Memory for Apu {
    fn readb(&mut self, addr: u16) -> u8 {
        if addr == 0x4015 {
            self.read_status()
        } else {
            // eprintln!("unhandled Apu readb at address 0x{:04X}", addr);
            0xFF
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            0x4000 => self.pulse1.write_control(val),
            0x4001 => self.pulse1.write_sweep(val),
            0x4002 => self.pulse1.write_timer_lo(val),
            0x4003 => self.pulse1.write_timer_hi(val),
            0x4004 => self.pulse2.write_control(val),
            0x4005 => self.pulse2.write_sweep(val),
            0x4006 => self.pulse2.write_timer_lo(val),
            0x4007 => self.pulse2.write_timer_hi(val),
            // 0x4008 => self.triangle.write_linear_counter(val),
            // 0x4009 => (), // Unused
            // 0x400A => self.triangle.write_timer_lo(val),
            // 0x400B => self.triangle.write_timer_hi(val),
            // 0x400C => self.noise.write_control(val),
            // 0x400D => (), // Unused
            // 0x400E => self.noise.write_period(val),
            // 0x400F => self.noise.write_length(val),
            // 0x4010 => self.dmc.write_frequency(val),
            // 0x4011 => self.dmc.write_raw(val),
            // 0x4012 => self.dmc.write_waveform(val),
            // 0x4013 => self.dmc.write_length(val),
            0x4015 => self.write_status(val),
            0x4017 => self.write_frame_counter(val),
            _ => (), // eprintln!("unhandled Apu writeb at address: 0x{:04X}", addr),
        }
    }
}

// https://wiki.nesdev.com/w/index.php/APU_Frame_Counter
struct FrameCounter {
    step: u8,     // The current step # of the 4-Step or 5-Step sequence
    counter: u16, // Counts CPU clocks until next step in the sequence
    mode: FCMode, // Either 4-Step sequence or 5-Step sequence
}
#[derive(PartialEq, Eq)]
enum FCMode {
    Step4,
    Step5,
}

#[derive(PartialEq, Eq)]
enum PulseChannel {
    One,
    Two,
}
struct Pulse {
    enabled: bool,
    length_enabled: bool,  // LengthCounter enable
    length_counter: u8,    // Entry into LENGTH_TABLE
    duty_cycle: u8,        // Select row in DUTY_TABLE
    duty_counter: u8,      // Select column in DUTY_TABLE
    freq_timer: u16,       // timer freq_counter reload value
    freq_counter: u16,     // Current frequency timer value
    channel: PulseChannel, // One or Two
    envelope: Envelope,
    sweep: Sweep,
}

struct Envelope {
    enabled: bool,
    loops: bool,
    reset: bool,
    volume: u8,
    constant_volume: u8,
    counter: u8,
}

struct Sweep {
    enabled: bool,
    reload: bool,
    negate: bool, // Treats PulseChannel 1 differently than PulseChannel 2
    timer: u8,    // counter reload value
    counter: u8,  // current timer value
    shift: u8,
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
            length_enabled: false,
            length_counter: 0u8,
            duty_cycle: 0u8,
            duty_counter: 0u8,
            freq_timer: 0u16,
            freq_counter: 0u16,
            channel,
            envelope: Envelope {
                enabled: false,
                loops: false,
                reset: false,
                volume: 0u8,
                constant_volume: 0u8,
                counter: 0u8,
            },
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

    fn clock(&mut self) {
        if self.freq_counter > 0 {
            self.freq_counter -= 1;
        } else {
            self.freq_counter = self.freq_timer;
            self.duty_counter = self.duty_counter.wrapping_add(1) & 7;
        }
    }
    fn clock_quarter_frame(&mut self) {
        let mut env = &mut self.envelope;
        if env.reset {
            env.reset = false;
            env.volume = 0x0F;
            env.counter = env.constant_volume;
        } else if env.counter > 0 {
            env.counter -= 1;
        } else {
            env.counter = env.constant_volume;
            if env.volume > 0 {
                env.volume -= 1;
            } else if env.loops {
                env.volume = 0x0F;
            }
        }
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

        if self.length_enabled && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    fn sweep_forcing_silence(&self) -> bool {
        let next_freq = self.freq_timer + (self.freq_timer >> self.sweep.shift);
        if self.freq_timer < 8 || (!self.sweep.negate && next_freq >= 0x800) {
            true
        } else {
            false
        }
    }

    fn output(&self) -> u8 {
        if Self::DUTY_TABLE[self.duty_cycle as usize][self.duty_counter as usize] != 0
            && self.length_counter != 0
            && !self.sweep_forcing_silence()
        {
            if self.envelope.enabled {
                self.envelope.volume
            } else {
                self.envelope.constant_volume
            }
        } else {
            0u8
        }
    }

    fn write_control(&mut self, val: u8) {
        self.duty_cycle = (val >> 6) & 0x03; // D7..D6
        self.length_enabled = (val >> 5) & 1 == 0; // !D5
        self.envelope.loops = (val >> 5) & 1 == 1; // D5
        self.envelope.enabled = (val >> 4) & 1 == 0; // !D4
        self.envelope.constant_volume = val & 0x0F; // D3..D0
    }
    fn write_sweep(&mut self, val: u8) {
        self.sweep.timer = (val >> 4) & 0x07; // D6..D4
        self.sweep.negate = (val >> 3) & 1 == 1; // D3
        self.sweep.shift = val & 0x07; // D2..D0
        self.sweep.enabled = ((val >> 7) & 1 == 1) && (self.sweep.shift != 0); // D7
        self.sweep.reload = true;
    }
    fn write_timer_lo(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0xFF00) | u16::from(val); // D7..D0
    }
    fn write_timer_hi(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0x00FF) | u16::from(val & 0x07) << 8; // D2..D0
        self.freq_counter = self.freq_timer;
        self.duty_counter = 0;
        self.envelope.reset = true;
        if self.enabled {
            self.length_counter = LENGTH_TABLE[(val >> 3) as usize]; // D7..D3
        }
    }
}

// struct Triangle {
//     sequencer_step: u8,
//     timer: u16,
//     timer_period: u16,
//     length_counter: LengthCounter,
//     linear_counter: LinearCounter,
// }

// struct Noise {
//     mode: bool,
//     timer: u16,
//     period: u16,
//     feedback: u16,
//     envelope: Envelope,
//     length_counter: LengthCounter,
// }

// // TODO Delta Modulation Channel
// struct DMC {
//     irq_enable: bool,
// }

// // https://wiki.nesdev.com/w/index.php/APU_Length_Counter
// struct LengthCounter {
//     enabled: bool,
//     counter: u8,
// }
// // https://wiki.nesdev.com/w/index.php/APU_Triangle
// struct LinearCounter {
//     enabled: bool,
//     reload: bool,
//     reload_value: u8,
//     counter: u8,
// }
// // https://wiki.nesdev.com/w/index.php/APU_Sweep
// struct Sweep {
//     enabled: bool,
//     reload: bool,
//     negate: bool,
//     divider: u8,
//     divider_period: u8,
//     shift_count: u8,
//     timer: u16,
//     period: u16,
//     channel: SqChan, // Channel 1 or 2 - 1 adds with one's complement; 2 adds with two's complement
// }
// struct Envelope {
//     enabled: bool,
//     start: bool,
//     looping: bool,
//     divider: u8,
//     period: u8,
//     volume: u8,
// }

// impl Triangle {
//     fn new() -> Self {
//         Self {
//             sequencer_step: 0u8,
//             timer: 0u16,
//             timer_period: 0u16,
//             length_counter: LengthCounter::new(),
//             linear_counter: LinearCounter::new(),
//         }
//     }

//     // Clocking

//     fn step(&mut self) {
//         if self.timer == 0 {
//             self.timer = self.timer_period;
//             if !self.silenced() {
//                 self.sequencer_step = (self.sequencer_step + 1) & 0x1F;
//             }
//         } else {
//             self.timer -= 1;
//         }
//     }
//     fn step_half_frame(&mut self) {
//         self.length_counter.step()
//     }
//     fn step_quarter_frame(&mut self) {
//         self.linear_counter.step()
//     }

//     // Getters

//     fn sample(&self) -> f64 {
//         let step = self.sequencer_step;
//         let sample = if step < 16 { 15 - step } else { step - 16 };
//         sample as f64
//     }
//     fn silenced(&self) -> bool {
//         self.length_counter.silenced() || self.linear_counter.silenced()
//     }
//     fn enabled(&self) -> bool {
//         self.length_counter.enabled()
//     }

//     // Setters

//     fn enable(&mut self) {
//         self.length_counter.enable();
//         self.linear_counter.enable();
//     }
//     fn disable(&mut self) {
//         self.length_counter.disable();
//         self.linear_counter.disable();
//     }

//     // Memory accesses

//     fn write_linear_counter(&mut self, val: u8) {
//         let flag = val & 0x80 > 0;
//         self.length_counter.enabled = !flag;
//         self.linear_counter.reload = flag;
//         self.linear_counter.reload_value = val & 0x7F;
//     }
//     fn write_timer_lo(&mut self, val: u8) {
//         self.timer_period = (self.timer_period & 0xFF00) | u16::from(val);
//     }
//     fn write_timer_hi(&mut self, val: u8) {
//         self.timer_period = (self.timer_period & 0xFF00) | (u16::from(val & 0x07) << 8);
//         self.length_counter.set_load(val >> 3);
//         self.linear_counter.reload = true;
//     }
// }

// impl Noise {
//     fn new() -> Self {
//         Self {
//             mode: false,
//             timer: 0u16,
//             period: 0u16,
//             feedback: 1u16,
//             envelope: Envelope::new(),
//             length_counter: LengthCounter::new(),
//         }
//     }

//     // Clocking

//     fn step(&mut self) {
//         if self.timer == 0 {
//             self.timer = self.period;
//             self.feedback = self.next_feedback();
//         } else {
//             self.timer -= 1;
//         }
//     }
//     fn step_half_frame(&mut self) {
//         self.length_counter.step();
//     }
//     fn step_quarter_frame(&mut self) {
//         self.envelope.step();
//     }

//     // Getters

//     fn sample(&self) -> f64 {
//         if self.feedback & 0x01 > 0 && !self.silenced() {
//             self.envelope.sample()
//         } else {
//             0f64
//         }
//     }
//     fn silenced(&self) -> bool {
//         self.length_counter.silenced()
//     }
//     fn enabled(&self) -> bool {
//         self.length_counter.enabled()
//     }
//     fn next_feedback(&self) -> u16 {
//         let mut feedback = self.feedback;
//         let newbit = if self.mode {
//             (feedback & 0x01) ^ ((feedback >> 6) & 0x01) // XOR bit 0 with bit 6
//         } else {
//             (feedback & 0x01) ^ ((feedback >> 1) & 0x01) // XOR bit 0 with bit 1
//         };
//         feedback |= if newbit > 0 { 1 << 14 } else { 0u16 };
//         feedback
//     }

//     // Setters

//     fn enable(&mut self) {
//         self.length_counter.enabled();
//     }
//     fn disable(&mut self) {
//         self.length_counter.disable();
//     }

//     // Memory accesses

//     fn write_control(&mut self, val: u8) {
//         self.length_counter.enabled = (val & 0x20) == 0;
//         self.envelope.write_control(val);
//     }
//     fn write_period(&mut self, val: u8) {
//         self.mode = val & 0x08 > 0;
//         self.period = NOISE_PERIOD_TABLE[(val & 0x0F) as usize];
//     }
//     fn write_length(&mut self, val: u8) {
//         self.length_counter.set_load(val >> 3);
//         self.envelope.reset();
//     }
// }

// impl DMC {
//     fn new() -> Self {
//         Self { irq_enable: true }
//     }

//     // Clocking

//     fn step(&mut self) {
//         // TODO
//     }
//     fn step_half_frame(&mut self) {
//         // TODO
//     }

//     // Getters

//     fn sample(&self) -> f64 {
//         0f64
//     }
//     fn enabled(&self) -> bool {
//         // TODO
//         true
//     }

//     // Setters

//     fn enable(&mut self) {
//         // TODO
//     }
//     fn disable(&mut self) {
//         // TODO
//     }

//     // Memory accesses

//     fn write_frequency(&mut self, val: u8) {
//         // TODO
//     }
//     fn write_raw(&mut self, val: u8) {
//         // TODO
//     }
//     fn write_waveform(&mut self, val: u8) {
//         // TODO
//     }
//     fn write_length(&mut self, val: u8) {
//         // TODO
//     }
// }

// impl FrameCounter {
//     pub fn new() -> Self {
//         Self {
//             irq_inhibit: false,
//             step: 0u16,
//             mode: Step4,
//         }
//     }

//     // Clocking

//     pub fn step(&mut self) {
//         self.step += 1;
//         let max_step = match self.mode {
//             Step4 => 29830,
//             Step5 => 37282,
//         };
//         if self.step >= max_step {
//             self.step -= max_step;
//         }
//     }
//     fn step_quarter_frame(&self) -> bool {
//         match (&self.mode, self.step) {
//             (_, 7457) | (_, 14913) | (_, 22371) | (Step4, 29829) | (Step5, 37281) => true,
//             _ => false,
//         }
//     }
//     fn step_half_frame(&self) -> bool {
//         match (&self.mode, self.step) {
//             (_, 14913) | (Step4, 29829) | (Step5, 37281) => true,
//             _ => false,
//         }
//     }
//     fn is_irq_frame(&self) -> bool {
//         if self.mode == Step4 && self.step > 29828 {
//             true
//         } else {
//             false
//         }
//     }

//     // Memory accesses

//     fn write_control(&mut self, val: u8) {
//         self.mode = if (val >> 7) & 1 == 0 { Step4 } else { Step5 }; // D7
//         self.irq_inhibit = (val >> 6) == 1;
//     }
// }

// impl LengthCounter {
//     fn new() -> Self {
//         Self {
//             enabled: true,
//             counter: 0u8,
//         }
//     }

//     // Clocking

//     fn step(&mut self) {
//         if self.counter > 0 && self.enabled {
//             self.counter -= 1;
//         }
//     }

//     // Getters

//     fn silenced(&self) -> bool {
//         self.counter == 0 || !self.enabled
//     }
//     fn enabled(&self) -> bool {
//         self.enabled
//     }

//     // Setters

//     fn enable(&mut self) {
//         self.enabled = true;
//     }
//     fn disable(&mut self) {
//         self.counter = 0;
//         self.enabled = false;
//     }
//     fn set_load(&mut self, val: u8) {
//         if self.enabled {
//             self.counter = LENGTH_TABLE[val as usize & 0x1F];
//         }
//     }
// }

// impl LinearCounter {
//     pub fn new() -> Self {
//         Self {
//             enabled: false,
//             reload: false,
//             reload_value: 0u8,
//             counter: 0u8,
//         }
//     }

//     // Clocking

//     pub fn step(&mut self) {
//         if self.reload {
//             self.counter = self.reload_value;
//         } else if self.counter > 0 {
//             self.counter -= 1;
//         }
//         if self.enabled {
//             self.reload = false;
//         }
//     }

//     // Getters

//     fn silenced(&self) -> bool {
//         self.counter == 0
//     }
//     fn enabled(&self) -> bool {
//         self.enabled
//     }

//     // Setters

//     fn enable(&mut self) {
//         self.enabled = true;
//     }
//     fn disable(&mut self) {
//         self.enabled = false;
//     }
// }

// impl Sweep {
//     pub fn new(channel: SqChan) -> Self {
//         Self {
//             enabled: false,
//             reload: false,
//             negate: false,
//             divider: 0u8,
//             divider_period: 0u8,
//             shift_count: 0u8,
//             timer: 0u16,
//             period: 0u16,
//             channel,
//         }
//     }

//     // Clocking

//     fn step(&mut self) {
//         if self.divider == 0 && self.enabled && !self.silenced() && self.shift_count != 0 {
//             self.period = self.target_period();
//         }
//         if self.divider == 0 || self.reload {
//             self.divider = self.divider_period;
//             self.reload = false;
//         } else {
//             self.divider -= 1;
//         }
//     }

//     // Getters

//     fn silenced(&self) -> bool {
//         self.period < 8 || self.target_period() > 0x7FF
//     }
//     fn target_period(&self) -> u16 {
//         let mut change = self.timer >> self.shift_count;
//         if self.negate {
//             change = self.negate(change);
//         }
//         self.period.wrapping_add(change)
//     }

//     // Memory accesses

//     fn write_control(&mut self, val: u8) {
//         self.shift_count = val & 0x07; // D2..D0
//         self.negate = (val >> 3) & 1 == 1; // D3
//         self.divider_period = (val >> 4) & 0x7; // D6..D4
//         self.enabled = ((val >> 7) & 1 == 1) && (self.shift_count != 0);
//         self.reload = true;
//     }

//     // Helpers

//     fn negate(&self, val: u16) -> u16 {
//         match self.channel {
//             SqChan::One => (-(val as i16)) as u16,
//             SqChan::Two => (-(val as i16) - 1) as u16,
//         }
//     }
// }

// impl Envelope {
//     pub fn new() -> Self {
//         Self {
//             enabled: false,
//             start: false,
//             looping: false,
//             divider: 0u8,
//             period: 0u8,
//             volume: 0u8,
//         }
//     }

//     // Clocking

//     fn step(&mut self) {
//         if self.start {
//             self.start = false;
//             self.period = 0x0F;
//             self.divider = self.volume;
//         } else if self.divider > 0 {
//             self.divider -= 1;
//         } else {
//             if self.period > 0 {
//                 self.period -= 1;
//             } else if self.looping {
//                 self.period = 0x0F;
//             }
//             self.divider = self.volume;
//         }
//     }

//     // Getters

//     fn sample(&self) -> f64 {
//         let sample = if self.enabled {
//             self.period
//         } else {
//             self.volume
//         };
//         sample as f64
//     }

//     // Setters

//     fn reset(&mut self) {
//         self.start = true;
//     }

//     // Memory accesses

//     fn write_control(&mut self, val: u8) {
//         self.period = val & 0x0F; // D3..D1
//         self.volume = val & 0x0F; // D3..D1
//         self.enabled = (val >> 4) & 1 == 1;
//         self.looping = (val >> 5) & 1 == 1;
//     }
// }

impl fmt::Debug for Apu {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "APU {{ cyc: {} }}", self.cycle)
    }
}
