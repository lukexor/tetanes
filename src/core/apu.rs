// Audio Processing Unit
#[derive(Default, Debug)]
pub struct APU {
    pub channel: f32,
    pub pulse1: Pulse,
    pub pulse2: Pulse,
    pub triangle: Triangle,
    pub noise: Noise,
    pub dmc: DMC,
    pub cycle: u64,
    pub frame_period: u8,
    pub frame_value: u8,
    pub frame_irq: bool,
}

impl APU {
    pub fn new() -> Self {
        APU {
            noise: Noise::new(),
            pulse1: Pulse::with_channel(1),
            pulse2: Pulse::with_channel(2),
            ..Default::default()
        }
    }
}

// Delta Modulation Channel
#[derive(Default, Debug)]
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

#[derive(Default, Debug)]
pub struct Pulse {
    pub enabled: bool,
    pub channel: u8,
    pub length_enabled: bool,
    pub length_value: u8,
    pub timer_period: u16,
    pub timer_value: u16,
    pub duty_mode: u8,
    pub duty_value: u8,
    pub sweep_reload: bool,
    pub sweep_enabled: bool,
    pub sweep_negate: bool,
    pub sweep_shift: u8,
    pub sweep_period: u8,
    pub sweep_value: u8,
    pub envelope_enabled: bool,
    pub envelope_loop: bool,
    pub envelope_start: bool,
    pub envelope_period: u8,
    pub envelope_value: u8,
    pub envelope_volume: u8,
    pub constant_volume: u8,
}

impl Pulse {
    pub fn with_channel(chan: u8) -> Self {
        Pulse {
            channel: chan,
            ..Default::default()
        }
    }
}

#[derive(Default, Debug)]
pub struct Triangle {
    pub enabled: bool,
    pub length_enabled: bool,
    pub length_value: u8,
    pub timer_period: u16,
    pub timer_value: u16,
    pub duty_value: u8,
    pub counter_period: u8,
    pub counter_value: u8,
    pub counter_reload: bool,
}

#[derive(Default, Debug)]
pub struct Noise {
    pub enabled: bool,
    pub mode: bool,
    pub shift_register: u16,
    pub length_enabled: bool,
    pub length_value: u8,
    pub timer_period: u16,
    pub timer_value: u16,
    pub envelope_enabled: bool,
    pub envelope_loop: bool,
    pub envelope_start: bool,
    pub envelope_period: u8,
    pub envelope_value: u8,
    pub envelope_volume: u8,
    pub constant_volume: u8,
}

impl Noise {
    pub fn new() -> Self {
        Noise {
            shift_register: 1,
            ..Default::default()
        }
    }
}
