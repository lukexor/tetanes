use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Filter {
    freq: f32,
    b0: f32,
    b1: f32,
    a1: f32,
    prev_in: f32,
    prev_out: f32,
}

impl Filter {
    pub fn low_pass(freq: f32, sample_rate: f32) -> Self {
        let cutoff = sample_rate / PI / freq;
        let a0i = 1.0 / (1.0 + cutoff);
        Self {
            freq,
            b0: a0i,
            b1: a0i,
            a1: (1.0 - cutoff) * a0i,
            prev_in: 0.0,
            prev_out: 0.0,
        }
    }

    pub fn high_pass(freq: f32, sample_rate: f32) -> Self {
        let cutoff = sample_rate / PI / freq;
        let a0i = 1.0 / (1.0 + cutoff);
        Self {
            freq,
            b0: cutoff * a0i,
            b1: -cutoff * a0i,
            a1: (1.0 - cutoff) * a0i,
            prev_in: 0.0,
            prev_out: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let cutoff = sample_rate / PI / self.freq;
        let a0i = 1.0 / (1.0 + cutoff);
        if (self.b0 - a0i).abs() < f32::EPSILON {
            self.b0 = a0i;
            self.b1 = a0i;
        } else {
            self.b0 = cutoff * a0i;
            self.b1 = -cutoff * a0i;
        }
        self.a1 = (1.0 - cutoff) * a0i;
    }

    pub fn apply(&mut self, sample: f32) -> f32 {
        let prev_in = self.prev_in;
        self.prev_in = sample;
        let out = self.b0.mul_add(sample, self.b1 * prev_in) - self.a1 * self.prev_out;
        self.prev_out = out;
        out
    }
}
