use serde::{Deserialize, Serialize};
pub use std::f32;

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Filter {
    freq: f32,
    b0: f32,
    b1: f32,
    a1: f32,
    prev_x: f32,
    prev_y: f32,
}

impl Filter {
    pub fn low_pass(freq: f32, sample_rate: f32) -> Self {
        let cutoff = sample_rate / f32::consts::PI / freq;
        let a0i = 1.0 / (1.0 + cutoff);
        Self {
            freq,
            b0: a0i,
            b1: a0i,
            a1: (1.0 - cutoff) * a0i,
            prev_x: 0.0,
            prev_y: 0.0,
        }
    }

    pub fn high_pass(freq: f32, sample_rate: f32) -> Self {
        let cutoff = sample_rate / f32::consts::PI / freq;
        let a0i = 1.0 / (1.0 + cutoff);
        Self {
            freq,
            b0: cutoff * a0i,
            b1: -cutoff * a0i,
            a1: (1.0 - cutoff) * a0i,
            prev_x: 0.0,
            prev_y: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let cutoff = sample_rate / f32::consts::PI / self.freq;
        let a0i = 1.0 / (1.0 + cutoff);
        if self.b0 == a0i {
            self.b0 = a0i;
            self.b1 = a0i;
        } else {
            self.b0 = cutoff * a0i;
            self.b1 = -cutoff * a0i;
        }
        self.a1 = (1.0 - cutoff) * a0i;
    }

    pub fn apply(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let prev_x = self.prev_x;
            self.prev_x = *sample;
            *sample = self.b0.mul_add(*sample, self.b1 * prev_x) - self.a1 * self.prev_y;
            self.prev_y = *sample;
        }
    }
}
