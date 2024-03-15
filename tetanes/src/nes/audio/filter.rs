use super::window_sinc::WindowSinc;

#[derive(Debug)]
#[must_use]
pub struct Filter {
    sinc: WindowSinc,
}

impl Filter {
    pub fn low_pass(sample_rate: f32, cutoff: f32, bandwidth: f32) -> Self {
        let sinc = WindowSinc::new(sample_rate, cutoff, bandwidth);
        Self { sinc }
    }

    pub fn high_pass(sample_rate: f32, cutoff: f32, bandwidth: f32) -> Self {
        let mut sinc = WindowSinc::new(sample_rate, cutoff, bandwidth);
        sinc.spectral_invert();
        Self { sinc }
    }

    #[must_use]
    pub fn apply(&self, sample: f32) -> f32 {
        let mut out = 0.0;
        for h in self.sinc.taps() {
            out += sample * h;
        }
        out
    }
}
