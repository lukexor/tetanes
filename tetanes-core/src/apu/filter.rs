use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    sincs: [WindowSinc; 3],
    input_rate: f32,
    output_rate: f32,
    resample_ratio: f32,
    sample_avg: f32,
    sample_count: f32,
    decim_fraction: f32,
    last_sample: Option<f32>,
}

impl Filter {
    pub fn new(input_rate: f32, output_rate: f32) -> Self {
        let resample_ratio = input_rate / output_rate;
        Self {
            sincs: [
                WindowSinc::high_pass(output_rate, 90.0, 1500.0),
                WindowSinc::high_pass(output_rate, 440.0, 1500.0),
                WindowSinc::low_pass(output_rate, 14_000.0, 1500.0),
            ],
            resample_ratio,
            input_rate,
            output_rate,
            sample_avg: 0.0,
            sample_count: 0.0,
            decim_fraction: resample_ratio,
            last_sample: None,
        }
    }

    pub fn set_input_rate(&mut self, input_rate: f32) {
        self.input_rate = input_rate;
        self.resample_ratio = self.input_rate / self.output_rate;
    }

    pub fn set_output_rate(&mut self, output_rate: f32) {
        self.output_rate = output_rate;
        self.resample_ratio = self.input_rate / self.output_rate;
    }

    pub fn add(&mut self, sample: f32) {
        self.sample_avg += sample;
        self.sample_count += 1.0;
        self.decim_fraction -= 1.0;
    }

    pub fn output(&mut self) -> Option<f32> {
        if self.decim_fraction < 1.0 {
            if self.sample_count > 0.0 {
                self.last_sample = Some(
                    self.sincs
                        .iter_mut()
                        .fold(self.sample_avg / self.sample_count, |s, sinc| sinc.apply(s)),
                );
            }
            self.sample_avg = 0.0;
            self.sample_count = 0.0;
            self.decim_fraction += self.resample_ratio;
            return self.last_sample;
        }
        None
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct WindowSinc {
    m: usize,
    fc: f32,
    bw: f32,
    taps: Vec<f32>,
    latency: usize,
}

impl WindowSinc {
    pub fn low_pass(sample_rate: f32, cutoff: f32, bandwidth: f32) -> Self {
        WindowSinc::new(sample_rate, cutoff, bandwidth)
    }

    pub fn high_pass(sample_rate: f32, cutoff: f32, bandwidth: f32) -> Self {
        let mut high_pass = WindowSinc::new(sample_rate, cutoff, bandwidth);
        high_pass.spectral_invert();
        high_pass
    }

    #[must_use]
    pub fn apply(&self, sample: f32) -> f32 {
        let mut out = 0.0;
        for h in &self.taps {
            out += sample * h;
        }
        out
    }

    /// Creates a new [`WindowSinc`] instance.
    ///
    /// # Panics
    ///
    /// Panics if `cutoff` or `bandwidth` ratio to `sample_rate` is greater than `0.5`.
    pub fn new(sample_rate: f32, cutoff: f32, bandwidth: f32) -> Self {
        let fc = cutoff / sample_rate;
        let bw = bandwidth / sample_rate;
        assert!(
            (0.0..=0.5).contains(&fc),
            "cutoff frequency can not be greater than 1/2 the sampling rate: {cutoff} / {sample_rate}",
        );
        assert!(
            (0.0..=0.5).contains(&bw),
            "transition bandwidth can not be greater than 1/2 the sampling rate: {bandwidth} / {sample_rate}",
        );

        let m = (4.0 / bw) as usize; // Approximation
        let latency = m / 2; // Middle sample of FIR

        let mut h = Self::blackman_window(m);

        // Apply window sinc filter
        let p = 2.0 * PI * fc;
        for (i, h) in h.iter_mut().enumerate() {
            let i = i as f32 - latency as f32;
            *h *= if i == 0.0 { p } else { (p * i).sin() / i };
        }

        // Normalize
        let sum_inv = 1.0 / h.iter().sum::<f32>();
        for h in &mut h {
            *h *= sum_inv;
        }

        Self {
            m,
            fc,
            bw,
            taps: h,
            latency,
        }
    }

    fn blackman_window(m: usize) -> Vec<f32> {
        let p1 = 2.0 * PI / m as f32;
        let p2 = 4.0 * PI / m as f32;

        // Force N to be symmetrical
        let n = if m % 2 == 0 { m + 1 } else { m };
        let mut h = vec![0.0; n];

        for (i, h) in h.iter_mut().enumerate() {
            let i = i as f32;
            *h = 0.42 - 0.5 * (p1 * i).cos() + 0.8 * (p2 * i).cos();
        }

        h
    }

    #[must_use]
    pub const fn taps(&self) -> &Vec<f32> {
        &self.taps
    }

    pub fn spectral_invert(&mut self) {
        let mut i = 1.0;
        for h in &mut self.taps {
            i *= -1.0;
            *h *= i;
        }
        self.taps[self.latency] += 1.0;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.taps.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.taps.is_empty()
    }

    #[must_use]
    pub const fn latency(&self) -> usize {
        self.latency
    }
}

impl std::fmt::Debug for WindowSinc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowSinc")
            .field("m", &self.m)
            .field("fc", &self.fc)
            .field("bw", &self.bw)
            .field("taps_len", &self.taps.len())
            .field("latency", &self.latency)
            .finish()
    }
}
