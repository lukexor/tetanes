use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub struct WindowSinc {
    m: usize,
    fc: f32,
    bw: f32,
    taps: Vec<f32>,
    latency: usize,
}

impl WindowSinc {
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
            "cutoff frequency can not be greater than 1/2 the sampling rate"
        );
        assert!(
            (0.0..=0.5).contains(&bw),
            "transition bandwidth can not be greater than 1/2 the sampling rate"
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

    #[inline]
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

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.taps.len()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.taps.is_empty()
    }

    #[inline]
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
