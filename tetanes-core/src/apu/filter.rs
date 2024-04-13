use crate::{
    common::{NesRegion, Sample},
    cpu::Cpu,
};
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

/// A trait for audio processing that consumes samples.
pub trait Consume {
    fn consume(&mut self, sample: f32);
}

/// Represents a digital filter with certain characteristics.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub enum FilterKind {
    Identity,
    HighPass,
    LowPass,
}

/// An infinite impulse response (IIR) filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Iir {
    pub alpha: f32,
    pub prev_output: f32,
    pub prev_input: f32,
    pub delta: f32,
    pub kind: FilterKind,
}

impl Iir {
    pub fn identity() -> Self {
        Self {
            alpha: 0.0,
            prev_output: 0.0,
            prev_input: 0.0,
            delta: 0.0,
            kind: FilterKind::Identity,
        }
    }

    pub fn high_pass(sample_rate: f32, cutoff: f32) -> Self {
        let period = 1.0 / sample_rate;
        let cutoff_period = 1.0 / cutoff;
        let alpha = cutoff_period / (cutoff_period + period);
        Self {
            alpha,
            prev_output: 0.0,
            prev_input: 0.0,
            delta: 0.0,
            kind: FilterKind::HighPass,
        }
    }

    pub fn low_pass(sample_rate: f32, cutoff: f32) -> Self {
        let period = 1.0 / sample_rate;
        let cutoff_period = 1.0 / (TAU * cutoff);
        let alpha = cutoff_period / (cutoff_period + period);
        Self {
            alpha,
            prev_output: 0.0,
            prev_input: 0.0,
            delta: 0.0,
            kind: FilterKind::LowPass,
        }
    }
}

impl Consume for Iir {
    fn consume(&mut self, sample: f32) {
        self.prev_output = self.output();
        self.delta = sample - self.prev_input;
        self.prev_input = sample;
    }
}

impl Sample for Iir {
    fn output(&self) -> f32 {
        match self.kind {
            FilterKind::Identity => self.prev_input,
            FilterKind::HighPass => self.alpha * self.prev_output + self.alpha * self.delta,
            FilterKind::LowPass => self.prev_output + self.alpha * self.delta,
        }
    }
}

/// A finite impulse response (FIR) filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Fir {
    pub kernel: Vec<f32>,
    pub inputs: Vec<f32>,
    pub input_index: usize,
    pub kind: FilterKind,
}

impl Fir {
    pub fn low_pass(sample_rate: f32, cutoff: f32, window_size: usize) -> Self {
        Self {
            kernel: windowed_sinc_kernel(sample_rate, cutoff, window_size),
            inputs: vec![0.0; window_size + 1],
            input_index: 0,
            kind: FilterKind::LowPass,
        }
    }
}

impl Consume for Fir {
    fn consume(&mut self, sample: f32) {
        self.inputs[self.input_index] = sample;
        self.input_index += 1;
        if self.input_index >= self.inputs.len() {
            self.input_index = 0;
        }
    }
}

impl Sample for Fir {
    fn output(&self) -> f32 {
        self.kernel
            .iter()
            .zip(self.inputs.iter().cycle().skip(self.input_index))
            .map(|(k, v)| k * v)
            .sum()
    }
}

/// Generate a windowed sinc kernel.
pub fn windowed_sinc_kernel(sample_rate: f32, cutoff: f32, window_size: usize) -> Vec<f32> {
    fn blackman_window(index: usize, window_size: usize) -> f32 {
        let i = index as f32;
        let m = window_size as f32;
        0.42 - 0.5 * ((TAU * i) / m).cos() + 0.08 * ((2.0 * TAU * i) / m).cos()
    }

    fn sinc(index: usize, fc: f32, window_size: usize) -> f32 {
        let i = index as f32;
        let m = window_size as f32;
        let shifted_index = i - (m / 2.0);
        if index == (window_size / 2) {
            TAU * fc
        } else {
            (TAU * fc * shifted_index).sin() / shifted_index
        }
    }

    fn normalize(input: Vec<f32>) -> Vec<f32> {
        let sum: f32 = input.iter().sum();
        input.into_iter().map(|x| x / sum).collect()
    }

    let fc = cutoff / sample_rate;
    let mut kernel = Vec::with_capacity(window_size);
    for i in 0..=window_size {
        kernel.push(sinc(i, fc, window_size) * blackman_window(i, window_size));
    }
    normalize(kernel)
}

/// Represents a digital audio filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub enum Filter {
    Iir(Iir),
    Fir(Fir),
}

impl Consume for Filter {
    fn consume(&mut self, sample: f32) {
        match self {
            Filter::Iir(iir) => iir.consume(sample),
            Filter::Fir(fir) => fir.consume(sample),
        }
    }
}

impl Sample for Filter {
    fn output(&self) -> f32 {
        match self {
            Filter::Iir(iir) => iir.output(),
            Filter::Fir(fir) => fir.output(),
        }
    }
}

impl From<Iir> for Filter {
    fn from(filter: Iir) -> Self {
        Self::Iir(filter)
    }
}

impl From<Fir> for Filter {
    fn from(filter: Fir) -> Self {
        Self::Fir(filter)
    }
}

/// Represents a filter with a given sampling period.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct SampledFilter {
    pub filter: Filter,
    pub sample_period: f32,
    pub period_counter: f32,
}

impl SampledFilter {
    pub fn new(filter: impl Into<Filter>, sample_rate: f32) -> Self {
        Self {
            filter: filter.into(),
            sample_period: 1.0 / sample_rate,
            period_counter: 0.0,
        }
    }
}

/// Represents a chain of filters for a given [`NesRegion`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterChain {
    pub region: NesRegion,
    pub dt: f32,
    pub filters: Vec<SampledFilter>,
}

impl FilterChain {
    pub fn new(region: NesRegion, output_rate: f32) -> Self {
        let clock_rate = Cpu::region_clock_rate(region);
        let intermediate_sample_rate = output_rate * 2.0 + (PI / 32.0);

        let mut filters = vec![SampledFilter::new(Iir::identity(), 1.0)];
        // first-order high-pass filter at 90 Hz
        filters.push(SampledFilter::new(
            Iir::high_pass(intermediate_sample_rate, 90.0),
            intermediate_sample_rate,
        ));
        // first-order high-pass filter at 440 Hz
        filters.push(SampledFilter::new(
            Iir::high_pass(intermediate_sample_rate, 440.0),
            intermediate_sample_rate,
        ));
        // first-order low-pass filter at 14 kHz
        filters.push(SampledFilter::new(
            Iir::low_pass(intermediate_sample_rate, 14000.0),
            intermediate_sample_rate,
        ));
        // TODO: Support famicom filter selection
        // // first-order high-pass filter at 37 Hz
        // filters.push(SampledFilter::new(
        //     Iir::high_pass(intermediate_sample_rate, 37.0),
        //     intermediate_sample_rate,
        // ));

        // high-quality low-pass filter
        let window_size = 60;
        let intermediate_cutoff = output_rate * 0.45;
        filters.push(SampledFilter::new(
            Fir::low_pass(intermediate_sample_rate, intermediate_cutoff, window_size),
            intermediate_sample_rate,
        ));

        Self {
            region,
            dt: 1.0 / clock_rate,
            filters,
        }
    }
}

impl Consume for FilterChain {
    fn consume(&mut self, sample: f32) {
        // Add sample to identity filter
        self.filters[0].filter.consume(sample);
        for i in 1..self.filters.len() {
            let prev = i - 1;
            let current = i;
            while self.filters[current].period_counter >= self.filters[current].sample_period {
                self.filters[current].period_counter -= self.filters[current].sample_period;
                let prev_output = self.filters[prev].filter.output();
                self.filters[current].filter.consume(prev_output);
            }
            self.filters[current].period_counter += self.dt;
        }
    }
}

impl Sample for FilterChain {
    fn output(&self) -> f32 {
        self.filters
            .last()
            .expect("no filters defined")
            .filter
            .output()
    }
}
