use crate::audio::window_sinc::WindowSinc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
enum Type {
    LowPass,
    HighPass,
    BandPass,
    BandReject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Filter {
    ty: Type,
    sinc: WindowSinc,
}

impl Filter {
    pub fn low_pass(sample_rate: f32, cutoff: f32, bandwidth: f32) -> Self {
        let sinc = WindowSinc::new(sample_rate, cutoff, bandwidth);
        Self {
            ty: Type::LowPass,
            sinc,
        }
    }

    pub fn high_pass(sample_rate: f32, cutoff: f32, bandwidth: f32) -> Self {
        let mut sinc = WindowSinc::new(sample_rate, cutoff, bandwidth);
        sinc.spectral_invert();
        Self {
            ty: Type::HighPass,
            sinc,
        }
    }

    #[inline]
    #[must_use]
    pub fn apply(&self, sample: f32) -> f32 {
        let mut out = 0.0;
        for h in self.sinc.taps() {
            out += sample * h;
        }
        out
    }
}
