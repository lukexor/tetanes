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
pub struct APU {
    channel: f32,
    pub sample_rate: f32,
    pub pulse1: Pulse,
    pub pulse2: Pulse,
    pub triangle: Triangle,
    pub noise: Noise,
    pub dmc: DMC,
    pub cycle: u64,
    pub frame_period: u8,
    pub frame_value: u8,
    pub frame_irq: bool,
    pulse_table: [f32; 31],
    tnd_table: [f32; 203],
}

impl APU {
    pub fn new() -> Self {
        let mut apu = Self {
            channel: 0.0,
            sample_rate: 0.0,
            pulse1: Pulse::with_channel(1),
            pulse2: Pulse::with_channel(2),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: DMC::new(),
            cycle: 0,
            frame_period: 0,
            frame_value: 0,
            frame_irq: false,
            pulse_table: [0.0; 31],
            tnd_table: [0.0; 203],
        };
        for i in 0..31 {
            apu.pulse_table[i] = 95.52 / (8128.0 / (i as f32) + 100.0);
        }
        for i in 0..203 {
            apu.tnd_table[i] = 163.67 / (24329.0 / (i as f32) + 100.0);
        }
        apu
    }

    pub fn send_sample(&mut self) {
        self.channel = self.output();
    }

    pub fn read_register(&self, addr: u16) -> u8 {
        if addr == 0x4015 {
            let mut result: u8 = 0;
            if self.pulse1.length_value > 0 {
                result |= 1;
            }
            if self.pulse2.length_value > 0 {
                result |= 2;
            }
            if self.triangle.length_value > 0 {
                result |= 4;
            }
            if self.noise.length_value > 0 {
                result |= 8;
            }
            if self.dmc.current_length > 0 {
                result |= 16;
            }
            result
        } else {
            panic!("unhandled APU register read at address 0x{:04X}", addr);
        }
    }

    pub fn write_register(&mut self, addr: u16, val: u8) {
        match addr {
            0x4000 => self.pulse1.write_control(val),
            0x4001 => self.pulse1.write_sweep(val),
            0x4002 => self.pulse1.write_timer_low(val),
            0x4003 => self.pulse1.write_timer_high(val),
            0x4004 => self.pulse2.write_control(val),
            0x4005 => self.pulse2.write_sweep(val),
            0x4006 => self.pulse2.write_timer_low(val),
            0x4007 => self.pulse2.write_timer_high(val),
            0x4008 => self.triangle.write_control(val),
            0x400A => self.triangle.write_timer_low(val),
            0x400B => self.triangle.write_timer_high(val),
            0x400C => self.noise.write_control(val),
            0x400E => self.noise.write_period(val),
            0x400F => self.noise.write_length(val),
            0x4010 => self.dmc.write_control(val),
            0x4011 => self.dmc.write_value(val),
            0x4012 => self.dmc.write_address(val),
            0x4013 => self.dmc.write_length(val),
            0x4015 => self.write_control(val),
            0x4017 => self.write_frame_counter(val),
            _ => panic!("unhandled APU register write at address: 0x{:04X}", addr),
        }
    }

    fn write_control(&mut self, val: u8) {
        self.pulse1.enabled = val & 1 == 1;
        self.pulse1.enabled = val & 2 == 2;
        self.triangle.enabled = val & 4 == 4;
        self.noise.enabled = val & 8 == 8;
        self.dmc.enabled = val & 16 == 16;
        if !self.pulse1.enabled {
            self.pulse1.length_value = 0;
        }
        if !self.pulse2.enabled {
            self.pulse2.length_value = 0;
        }
        if !self.triangle.enabled {
            self.triangle.length_value = 0;
        }
        if !self.noise.enabled {
            self.noise.length_value = 0;
        }
        if !self.dmc.enabled {
            self.dmc.current_length = 0;
        } else if self.dmc.current_length == 0 {
            self.dmc.restart();
        }
    }

    fn write_frame_counter(&mut self, val: u8) {
        self.frame_period = 4 + ((val >> 7) & 1);
        self.frame_irq = (val >> 6) & 1 == 0;
        if self.frame_period == 5 {
            self.step_envelope();
            self.step_sweep();
            self.step_length();
        }
    }

    pub fn step_envelope(&mut self) {
        self.pulse1.step_envelope();
        self.pulse2.step_envelope();
        self.triangle.step_counter();
        self.noise.step_envelope();
    }

    pub fn step_sweep(&mut self) {
        self.pulse1.step_sweep();
        self.pulse2.step_sweep();
    }

    pub fn step_length(&mut self) {
        self.pulse1.step_length();
        self.pulse2.step_length();
        self.triangle.step_length();
        self.noise.step_length();
    }

    fn output(&self) -> f32 {
        let pulse1 = self.pulse1.output();
        let pulse2 = self.pulse2.output();
        let triangle = f32::from(self.triangle.output());
        let noise = f32::from(self.noise.output());
        let dmc = f32::from(self.dmc.output());
        let pulse_out = self.pulse_table[(pulse1 + pulse2) as usize];
        let tnd_out = self.tnd_table[(3.0 * triangle + 2.0 * noise + dmc) as usize];
        pulse_out + tnd_out
    }
}

impl Default for APU {
    fn default() -> Self {
        Self::new()
    }
}

// Delta Modulation Channel
#[derive(Default)]
pub struct DMC {
    pub enabled: bool,
    pub value: u8,
    pub sample_address: u16,
    pub sample_length: u16,
    pub current_address: u16,
    pub current_length: u16,
    pub shift_register: u8,
    pub bit_count: u8,
    pub tick_period: u8,
    pub tick_value: u8,
    pub loops: bool,
    pub irq: bool,
}

impl DMC {
    fn new() -> Self {
        Default::default()
    }

    fn write_control(&mut self, val: u8) {
        self.irq = val & 0x80 == 0x80;
        self.loops = val & 0x40 == 0x40;
        self.tick_period = DMC_TABLE[(val & 0x0F) as usize];
    }

    fn write_value(&mut self, val: u8) {
        self.value = val & 0x7F;
    }

    fn write_address(&mut self, val: u8) {
        self.sample_address = 0xC000 | (u16::from(val) << 6);
    }

    fn write_length(&mut self, val: u8) {
        self.sample_length = (u16::from(val) << 4) | 1;
    }

    pub fn restart(&mut self) {
        self.current_address = self.sample_address;
        self.current_length = self.sample_length;
    }

    fn output(&self) -> u8 {
        self.value
    }
}

#[derive(Default)]
pub struct Pulse {
    enabled: bool,
    channel: u8,
    length_enabled: bool,
    length_value: u8,
    timer_period: u16,
    timer_value: u16,
    duty_mode: u8,
    duty_value: u8,
    sweep_reload: bool,
    sweep_enabled: bool,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_period: u8,
    sweep_value: u8,
    envelope_enabled: bool,
    envelope_loop: bool,
    envelope_start: bool,
    envelope_period: u8,
    envelope_value: u8,
    envelope_volume: u8,
    constant_volume: u8,
}

impl Pulse {
    fn with_channel(chan: u8) -> Self {
        Self {
            channel: chan,
            ..Default::default()
        }
    }

    fn write_control(&mut self, val: u8) {
        self.duty_mode = (val >> 6) & 3;
        self.length_enabled = (val >> 5) & 1 == 0;
        self.envelope_loop = (val >> 5) & 1 == 1;
        self.envelope_enabled = (val >> 4) & 1 == 0;
        self.envelope_period = val & 15;
        self.constant_volume = val & 15;
        self.envelope_start = true;
    }

    fn write_sweep(&mut self, val: u8) {
        self.sweep_enabled = (val >> 7) & 1 == 1;
        self.sweep_period = ((val >> 4) & 7) + 1;
        self.sweep_negate = (val >> 3) & 1 == 1;
        self.sweep_shift = val & 7;
        self.sweep_reload = true;
    }

    fn write_timer_low(&mut self, val: u8) {
        self.timer_period = (self.timer_period & 0xFF00) | u16::from(val);
    }

    fn write_timer_high(&mut self, val: u8) {
        self.length_value = LENGTH_TABLE[(val >> 3) as usize];
        self.timer_period = (self.timer_period & 0x00FF) | (u16::from(val & 7) << 8);
        self.envelope_start = true;
        self.duty_value = 0;
    }

    pub fn step_timer(&mut self) {
        if self.timer_value == 0 {
            self.timer_value = self.timer_period;
            self.duty_value = (self.duty_value + 1) % 8;
        } else {
            self.timer_value -= 1;
        }
    }

    fn step_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_volume = 15;
            self.envelope_value = self.envelope_period;
            self.envelope_start = false;
        } else if self.envelope_value > 0 {
            self.envelope_value -= 1;
        } else {
            if self.envelope_volume > 0 {
                self.envelope_volume -= 1;
            } else if self.envelope_loop {
                self.envelope_volume = 15;
            }
            self.envelope_value = self.envelope_period;
        }
    }

    fn step_sweep(&mut self) {
        if self.sweep_reload {
            if self.sweep_enabled && self.sweep_value == 0 {
                self.sweep();
            }
            self.sweep_value = self.sweep_period;
            self.sweep_reload = false;
        } else if self.sweep_value > 0 {
            self.sweep_value -= 1;
        } else {
            if self.sweep_enabled {
                self.sweep();
            }
            self.sweep_value = self.sweep_period;
        }
    }

    fn sweep(&mut self) {
        let delta = self.timer_period >> self.sweep_shift;
        if self.sweep_negate {
            self.timer_period -= delta;
            if self.channel == 1 {
                self.timer_period -= 1;
            }
        } else {
            self.timer_period += delta;
        }
    }

    fn step_length(&mut self) {
        if self.length_enabled && self.length_value > 0 {
            self.length_value -= 1;
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled || self.length_value == 0 {
            return 0;
        }
        if DUTY_TABLE[self.duty_mode as usize][self.duty_value as usize] == 0 {
            return 0;
        }
        if self.timer_period < 8 || self.timer_period > 0x7FF {
            return 0;
        }
        if self.envelope_enabled {
            self.envelope_volume
        } else {
            self.constant_volume
        }
    }
}

#[derive(Default)]
pub struct Triangle {
    enabled: bool,
    length_enabled: bool,
    length_value: u8,
    timer_period: u16,
    timer_value: u16,
    duty_value: u8,
    counter_period: u8,
    counter_value: u8,
    counter_reload: bool,
}

impl Triangle {
    fn new() -> Self {
        Default::default()
    }

    fn write_control(&mut self, val: u8) {
        self.length_enabled = (val >> 7) & 1 == 0;
        self.counter_period = val & 0x7F;
    }

    fn write_timer_low(&mut self, val: u8) {
        self.timer_period = (self.timer_period & 0xFF00) | u16::from(val);
    }

    fn write_timer_high(&mut self, val: u8) {
        self.length_value = LENGTH_TABLE[(val >> 3) as usize];
        self.timer_period = (self.timer_period & 0x00FF) | (u16::from(val & 7) << 8);
        self.timer_value = self.timer_period;
        self.counter_reload = true;
    }

    pub fn step_timer(&mut self) {
        if self.timer_value == 0 {
            self.timer_value = self.timer_period;
            if self.length_value > 0 && self.counter_value > 0 {
                self.duty_value = (self.duty_value + 1) % 32;
            }
        } else {
            self.timer_value -= 1;
        }
    }

    fn step_length(&mut self) {
        if self.length_enabled && self.length_value > 0 {
            self.length_value -= 1;
        }
    }

    fn step_counter(&mut self) {
        if self.counter_reload {
            self.counter_value = self.counter_period;
        } else if self.counter_value > 0 {
            self.counter_value -= 1;
        }
        if self.length_enabled {
            self.counter_reload = false;
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled || self.length_value == 0 {
            return 0;
        }
        if self.counter_value != 0 {
            TRIANGLE_TABLE[self.duty_value as usize]
        } else {
            0
        }
    }
}

#[derive(Default)]
pub struct Noise {
    enabled: bool,
    mode: bool,
    shift_register: u16,
    length_enabled: bool,
    length_value: u8,
    timer_period: u16,
    timer_value: u16,
    envelope_enabled: bool,
    envelope_loop: bool,
    envelope_start: bool,
    envelope_period: u8,
    envelope_value: u8,
    envelope_volume: u8,
    constant_volume: u8,
}

impl Noise {
    fn new() -> Self {
        Self {
            shift_register: 1,
            ..Default::default()
        }
    }

    fn write_control(&mut self, val: u8) {
        self.length_enabled = (val >> 5) & 1 == 0;
        self.envelope_loop = (val >> 5) & 1 == 1;
        self.envelope_enabled = (val >> 4) & 1 == 0;
        self.envelope_period = val & 15;
        self.constant_volume = val & 15;
        self.envelope_start = true;
    }

    fn write_period(&mut self, val: u8) {
        self.mode = val & 0x80 == 0x80;
        self.timer_period = NOISE_TABLE[(val & 0x0F) as usize];
    }

    fn write_length(&mut self, val: u8) {
        self.length_value = LENGTH_TABLE[(val >> 3) as usize];
        self.envelope_start = true;
    }

    pub fn step_timer(&mut self) {
        if self.timer_value == 0 {
            self.timer_value = self.timer_period;
            let shift = if self.mode { 6 } else { 1 };
            let bit1 = self.shift_register & 1;
            let bit2 = (self.shift_register >> shift) & 1;
            self.shift_register >>= 1;
            self.shift_register |= (bit1 ^ bit2) << 14;
        } else {
            self.timer_value -= 1;
        }
    }

    fn step_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_volume = 15;
            self.envelope_value = self.envelope_period;
            self.envelope_start = false;
        } else if self.envelope_value > 0 {
            self.envelope_value -= 1;
        } else {
            if self.envelope_volume > 0 {
                self.envelope_volume -= 1;
            } else if self.envelope_loop {
                self.envelope_volume = 15;
            }
            self.envelope_value = self.envelope_period;
        }
    }

    fn step_length(&mut self) {
        if self.length_enabled && self.length_value > 0 {
            self.length_value -= 1;
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled || self.length_value == 0 {
            return 0;
        }
        if self.shift_register & 1 == 1 {
            return 0;
        }
        if self.envelope_enabled {
            self.envelope_volume
        } else {
            self.constant_volume
        }
    }
}

#[cfg(test)]
mod tests {}
