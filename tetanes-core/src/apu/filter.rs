//! Digital filters for the [`Apu`](crate::apu::Apu).
//!
//! See <https://www.nesdev.org/wiki/APU_Mixer>

// The APU's internal wiring: channel state, dividers and filter taps, whose meaning is the
// hardware's rather than this crate's. Public for embedders and debuggers, not a stable surface -
// see the module docs on `apu`.
#![allow(missing_docs)]

use crate::{common::NesRegion, cpu::Cpu};
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

/// Represents a digital filter with certain characteristics.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub enum FilterKind {
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
    pub fn consume(&mut self, sample: f32) {
        self.prev_output = self.output();
        self.delta = sample - self.prev_input;
        self.prev_input = sample;
    }
    pub fn output(&self) -> f32 {
        match self.kind {
            FilterKind::HighPass => self.alpha * self.prev_output + self.alpha * self.delta,
            FilterKind::LowPass => self.prev_output + self.alpha * self.delta,
        }
    }
}

/// A finite impulse response (FIR) filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Fir {
    pub kernel: Box<[f32]>,
    pub inputs: Box<[f32]>,
    pub input_index: usize,
    pub kind: FilterKind,
}

impl Fir {
    pub fn low_pass(sample_rate: f32, cutoff: f32, window_size: usize) -> Self {
        Self {
            kernel: windowed_sinc_kernel(sample_rate, cutoff, window_size),
            inputs: vec![0.0; window_size + 1].into(),
            input_index: 0,
            kind: FilterKind::LowPass,
        }
    }
    pub fn consume(&mut self, sample: f32) {
        self.inputs[self.input_index] = sample;
        self.input_index += 1;
        if self.input_index >= self.inputs.len() {
            self.input_index = 0;
        }
    }
    pub fn output(&self) -> f32 {
        // `inputs` is a ring buffer whose write cursor is `input_index`. The cursor points at the
        // oldest sample, so the convolution splits into two straight dot products: the samples
        // from the cursor to the end, then those before it.
        let idx = self.input_index.min(self.inputs.len());
        let (recent, oldest) = self.inputs.split_at(idx);
        let split = oldest.len().min(self.kernel.len());
        let (kernel_oldest, kernel_recent) = self.kernel.split_at(split);

        dot(kernel_oldest, oldest) + dot(kernel_recent, recent)
    }
}

/// Dot product of two slices, summed with four independent accumulators.
///
/// Float addition is not associative, so LLVM may not split a single running accumulator on its
/// own; with one the loop is bound by add latency rather than throughput. Four partial sums let
/// the multiplies and adds pipeline, and let the loop auto-vectorize.
///
/// Deliberately avoids [`f32::mul_add`]: it guarantees a single rounding, which without hardware
/// FMA support means a call into libm's `fmaf`. The default `x86-64` target has no FMA, and no
/// `target-feature` is set for this crate, so `mul_add` here was measurably more expensive than a
/// separate multiply and add.
#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let (a, b) = (&a[..len], &b[..len]);

    let mut acc = [0.0f32; 4];
    let mut a_chunks = a.chunks_exact(4);
    let mut b_chunks = b.chunks_exact(4);
    for (x, y) in a_chunks.by_ref().zip(b_chunks.by_ref()) {
        acc[0] += x[0] * y[0];
        acc[1] += x[1] * y[1];
        acc[2] += x[2] * y[2];
        acc[3] += x[3] * y[3];
    }

    let mut sum = (acc[0] + acc[1]) + (acc[2] + acc[3]);
    for (x, y) in a_chunks.remainder().iter().zip(b_chunks.remainder()) {
        sum += x * y;
    }
    sum
}

/// Generate a windowed sinc kernel.
pub fn windowed_sinc_kernel(sample_rate: f32, cutoff: f32, window_size: usize) -> Box<[f32]> {
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

    fn normalize(input: Box<[f32]>) -> Box<[f32]> {
        let sum: f32 = input.iter().sum();
        input.into_iter().map(|x| x / sum).collect()
    }

    let fc = cutoff / sample_rate;
    let mut kernel = Vec::with_capacity(window_size);
    for i in 0..=window_size {
        kernel.push(sinc(i, fc, window_size) * blackman_window(i, window_size));
    }
    normalize(kernel.into())
}

/// Represents a chain of filters for a given [`NesRegion`].
///
/// The chain runs at two rates, not six. `low_pass` samples at the CPU clock rate, so it consumes
/// every call; the four downstream stages all sample at the same intermediate rate, so one counter
/// drives all four.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterChain {
    pub region: NesRegion,
    /// One CPU cycle in seconds, which is what each [`FilterChain::consume`] adds to
    /// `period_counter`.
    pub dt: f32,
    /// Seconds between intermediate-rate samples.
    pub sample_period: f32,
    /// Time accumulated toward the next intermediate-rate sample.
    pub period_counter: f32,
    /// Whether a sample has been consumed yet. See [`FilterChain::consume`].
    pub primed: bool,
    /// Anti-aliasing low-pass, at the CPU clock rate.
    pub low_pass: Iir,
    /// First-order high-pass at 90 Hz.
    pub high_pass_90: Iir,
    /// First-order high-pass at 440 Hz.
    pub high_pass_440: Iir,
    /// First-order low-pass at 14 kHz.
    pub low_pass_14k: Iir,
    /// High-quality windowed-sinc low-pass, the chain's output.
    pub fir: Fir,
}

impl FilterChain {
    pub fn new(region: NesRegion, output_rate: f32) -> Self {
        let clock_rate = Cpu::region_clock_rate(region);
        let intermediate_sample_rate = output_rate * 2.0 + (PI / 32.0);
        let intermediate_cutoff = output_rate * 0.4;

        // TODO: Support famicom filter selection
        // // first-order high-pass filter at 37 Hz
        // Iir::high_pass(intermediate_sample_rate, 37.0),
        Self {
            region,
            dt: 1.0 / clock_rate,
            sample_period: 1.0 / intermediate_sample_rate,
            period_counter: 0.0,
            primed: false,
            low_pass: Iir::low_pass(clock_rate, intermediate_cutoff),
            high_pass_90: Iir::high_pass(intermediate_sample_rate, 90.0),
            high_pass_440: Iir::high_pass(intermediate_sample_rate, 440.0),
            low_pass_14k: Iir::low_pass(intermediate_sample_rate, 14000.0),
            fir: {
                let window_size = 160;
                let cutoff = output_rate * 0.45;
                Fir::low_pass(intermediate_sample_rate, cutoff, window_size)
            },
        }
    }

    /// Consume one CPU cycle's mixed output.
    ///
    /// The hot path is a compare and an add: the intermediate stages are due about once every 19
    /// cycles at a 48 kHz output rate, and only then is anything past `low_pass` touched.
    pub fn consume(&mut self, sample: f32) {
        // The clock-rate stage used to be driven by a counter of its own whose period was exactly
        // `dt`, so it fired on every call but the first. `primed` keeps that: the stages are
        // recursive, so feeding a fresh chain one extra leading sample shifts every sample after
        // it, and the rewrite was verified bit-identical to the six-counter chain it replaced.
        if self.primed {
            self.low_pass.consume(sample);
        } else {
            self.primed = true;
        }

        // `while` rather than `if` only for an output rate high enough to make the intermediate
        // period shorter than a CPU cycle, which no supported rate is.
        while self.period_counter >= self.sample_period {
            self.period_counter -= self.sample_period;
            self.resample();
        }
        self.period_counter += self.dt;
    }

    /// Run the intermediate-rate stages, each consuming the one above it.
    fn resample(&mut self) {
        self.high_pass_90.consume(self.low_pass.output());
        self.high_pass_440.consume(self.high_pass_90.output());
        self.low_pass_14k.consume(self.high_pass_440.output());
        self.fir.consume(self.low_pass_14k.output());
    }

    pub fn output(&self) -> f32 {
        self.fir.output()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Naive circular convolution, matching the definition `Fir::output` optimizes.
    fn reference_output(fir: &Fir) -> f32 {
        let mut sum = 0.0;
        for (i, k) in fir.kernel.iter().enumerate() {
            sum += k * fir.inputs[(fir.input_index + i) % fir.inputs.len()];
        }
        sum
    }

    #[test]
    fn fir_output_matches_reference() {
        let mut fir = Fir::low_pass(48000.0, 20000.0, 160);
        assert_eq!(
            fir.kernel.len(),
            fir.inputs.len(),
            "kernel and ring buffer must be the same length"
        );

        // Walk the write cursor all the way around the ring so both the split and the
        // non-multiple-of-four remainder paths are exercised at every offset.
        for i in 0..=fir.inputs.len() {
            fir.consume((i as f32 * 0.37).sin());

            let actual = fir.output();
            let expected = reference_output(&fir);
            assert!(
                (actual - expected).abs() < 1e-5,
                "index {}: {actual} != {expected}",
                fir.input_index
            );
        }
    }

    #[test]
    fn dot_handles_empty_and_short_slices() {
        assert_eq!(dot(&[], &[]), 0.0);
        assert_eq!(dot(&[1.0, 2.0], &[]), 0.0);
        assert_eq!(dot(&[2.0, 3.0], &[4.0, 5.0]), 23.0);
        // Longer than one chunk, with a remainder.
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let b = [1.0; 7];
        assert_eq!(dot(&a, &b), 28.0);
    }
}
