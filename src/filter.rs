//! Contains the Filter Trait for both High Pass and Low Pass filters

use std::f64::consts;

/// Filter trait
pub trait Filter {
    fn process(&mut self, sample: f32) -> f32;
}

/// High Pass Filter
pub struct HiPassFilter {
    b0: f32,
    b1: f32,
    a1: f32,
    prev_x: f32,
    prev_y: f32,
}

impl HiPassFilter {
    pub fn new(freq: f64, sample_rate: f64) -> Self {
        let c = (sample_rate / consts::PI / freq) as f32;
        let a0i = 1.0 / (1.0 + c);
        Self {
            b0: c * a0i,
            b1: -c * a0i,
            a1: (1.0 - c) * a0i,
            prev_x: 0.0,
            prev_y: 0.0,
        }
    }
}

impl Filter for HiPassFilter {
    fn process(&mut self, sample: f32) -> f32 {
        let y = self.b0 * sample + self.b1 * self.prev_x - self.a1 * self.prev_y;
        self.prev_y = y;
        self.prev_x = sample;
        y
    }
}

/// Low Pass Filter
pub struct LoPassFilter {
    b0: f32,
    b1: f32,
    a1: f32,
    prev_x: f32,
    prev_y: f32,
}

impl LoPassFilter {
    pub fn new(freq: f64, sample_rate: f64) -> Self {
        let c = (sample_rate / consts::PI / freq) as f32;
        let a0i = 1.0 / (1.0 + c);
        Self {
            b0: a0i,
            b1: a0i,
            a1: (1.0 - c) * a0i,
            prev_x: 0.0,
            prev_y: 0.0,
        }
    }
}

impl Filter for LoPassFilter {
    fn process(&mut self, sample: f32) -> f32 {
        let y = self.b0 * sample + self.b1 * self.prev_x - self.a1 * self.prev_y;
        self.prev_y = y;
        self.prev_x = sample;
        y
    }
}
