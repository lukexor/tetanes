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

/// Flushes a value too small to hear down to zero.
///
/// The high-pass stages decay exponentially toward silence at ~0.1% a sample, so after every note
/// they spend thousands of samples in the denormal range, and everything downstream inherits it -
/// including the whole of [`Fir`]'s tap ring, every entry of which is multiplied on every output
/// sample. On x86 each denormal operand costs a microcode assist of ~100 cycles, so this is not a
/// rounding nicety: **measured at 49M assists over a 1440-frame run, around 4 billion cycles**,
/// which was larger than the entire cost of mixing.
///
/// `1e-20` is ~400 dB below full scale, so nothing audible is being discarded; the threshold is
/// far above the denormal boundary (`1.2e-38`) so no denormal is ever produced.
#[inline(always)]
fn flush(value: f32) -> f32 {
    if value.abs() < 1e-20 { 0.0 } else { value }
}

/// A one-pole infinite impulse response (IIR) high-pass filter.
///
/// `y[n] = alpha * (y[n-1] + x[n] - x[n-1])`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct HighPass {
    pub alpha: f32,
    pub prev_output: f32,
    pub prev_input: f32,
    pub delta: f32,
}

impl HighPass {
    pub fn new(sample_rate: f32, cutoff: f32) -> Self {
        let period = 1.0 / sample_rate;
        let cutoff_period = 1.0 / cutoff;
        Self {
            alpha: cutoff_period / (cutoff_period + period),
            prev_output: 0.0,
            prev_input: 0.0,
            delta: 0.0,
        }
    }

    pub fn consume(&mut self, sample: f32) {
        self.prev_output = flush(self.output());
        self.delta = flush(sample - self.prev_input);
        self.prev_input = sample;
    }

    pub fn output(&self) -> f32 {
        self.alpha * self.prev_output + self.alpha * self.delta
    }
}

/// A finite impulse response (FIR) filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Fir {
    pub kernel: Box<[f32]>,
    pub inputs: Box<[f32]>,
    pub input_index: usize,
}

impl Fir {
    pub fn low_pass(sample_rate: f32, cutoff: f32, window_size: usize) -> Self {
        Self {
            kernel: windowed_sinc_kernel(sample_rate, cutoff, window_size),
            inputs: vec![0.0; window_size + 1].into(),
            input_index: 0,
        }
    }
    pub fn consume(&mut self, sample: f32) {
        self.inputs[self.input_index] = flush(sample);
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
/// Every stage runs at one intermediate rate, a little over twice the output rate, and
/// [`FilterChain::consume`] is called only on the CPU cycles where that rate is due -
/// [`FilterChain::cycles_until_due`] says how far away the next one is, and
/// [`FilterChain::skip`] walks past the cycles in between.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterChain {
    pub region: NesRegion,
    /// CPU cycles between intermediate-rate samples, in [`FilterChain::FRAC_BITS`] fixed point.
    pub cycles_per_sample: i64,
    /// Fixed-point CPU cycles until the next intermediate-rate sample, which is due once this is
    /// not positive.
    pub period_counter: i64,
    /// Scales the incoming sample; see [`FilterChain::new`].
    pub input_gain: f32,
    /// First-order high-pass at 90 Hz.
    pub high_pass_90: HighPass,
    /// First-order high-pass at 440 Hz.
    pub high_pass_440: HighPass,
    /// High-quality windowed-sinc low-pass, the chain's output.
    pub fir: Fir,
}

impl FilterChain {
    /// Fractional bits in `cycles_per_sample` and `period_counter`.
    ///
    /// The counter is fixed point rather than the seconds-and-`dt` floats it used to be so that
    /// skipping `n` cycles in one step is *bit-identical* to stepping one cycle `n` times. As
    /// floats the two drifted apart - each accumulated rounding differently - and after a few
    /// thousand cycles a sample landed on a different cycle in the skipped path than in the walked
    /// one, permanently offsetting everything downstream.
    pub const FRAC_BITS: u32 = 32;
    /// One CPU cycle, in fixed point.
    const ONE_CYCLE: i64 = 1 << Self::FRAC_BITS;

    pub fn new(region: NesRegion, output_rate: f32) -> Self {
        let clock_rate = Cpu::region_clock_rate(region);
        let intermediate_sample_rate = output_rate * 2.0 + (PI / 32.0);
        let intermediate_cutoff = output_rate * 0.4;

        // `input_gain` was two `Iir` stages, a clock-rate anti-aliasing low-pass at
        // `intermediate_cutoff` and a 14 kHz low-pass at the intermediate rate. Both were
        // degenerate. That `Iir`'s low-pass mode computed `y[n] = y[n-1] + alpha*(x[n] - x[n-1])`,
        // which telescopes to `y[n] = alpha*x[n]` exactly - not approximately, and verified
        // bit-identical over 200k samples - so each was a constant gain and neither filtered
        // anything. High-passes are linear, so the two gains fold into one multiply at the input
        // and the output is unchanged.
        //
        // The clock-rate one is why `consume` used to be called every CPU cycle: ~29,800 calls a
        // frame to multiply by a constant that was then read about 1,470 times. Deleting it is
        // what lets the chain be driven by `cycles_until_due` instead.
        //
        // The consequence to keep in mind is that **there is now no anti-aliasing ahead of the
        // clock-rate -> intermediate-rate decimation**, and there never was; `fir` only guards the
        // second decimation. Doing it properly means band-limited synthesis from channel deltas
        // rather than a filter on a per-cycle stream, which is a design change, not a stage.
        let alpha = |sample_rate: f32, cutoff: f32| {
            let cutoff_period = 1.0 / (TAU * cutoff);
            cutoff_period / (cutoff_period + 1.0 / sample_rate)
        };

        // TODO: Support famicom filter selection
        // // first-order high-pass filter at 37 Hz
        // HighPass::new(intermediate_sample_rate, 37.0),
        let cycles_per_sample = (f64::from(clock_rate) / f64::from(intermediate_sample_rate)
            * Self::ONE_CYCLE as f64) as i64;
        Self {
            region,
            cycles_per_sample,
            period_counter: cycles_per_sample,
            input_gain: alpha(clock_rate, intermediate_cutoff)
                * alpha(intermediate_sample_rate, 14_000.0),
            high_pass_90: HighPass::new(intermediate_sample_rate, 90.0),
            high_pass_440: HighPass::new(intermediate_sample_rate, 440.0),
            fir: {
                let window_size = 160;
                let cutoff = output_rate * 0.45;
                Fir::low_pass(intermediate_sample_rate, cutoff, window_size)
            },
        }
    }

    /// CPU cycles until the chain next needs a sample, or 0 if one is due now.
    ///
    /// The caller mixes a sample only on the cycles this reports as due, and [`FilterChain::skip`]s
    /// the rest.
    #[inline]
    #[must_use]
    pub const fn cycles_until_due(&self) -> usize {
        if self.period_counter <= 0 {
            return 0;
        }
        // Ceiling division: a sample due part-way through a cycle is taken on that cycle. The
        // counter is positive here, so the shift is the whole of it.
        ((self.period_counter + Self::ONE_CYCLE - 1) >> Self::FRAC_BITS) as usize
    }

    /// Walk past `cycles` CPU cycles that [`FilterChain::cycles_until_due`] said carry no sample.
    #[inline]
    pub const fn skip(&mut self, cycles: usize) {
        self.period_counter -= cycles as i64 * Self::ONE_CYCLE;
    }

    /// Consume the mixed output of a CPU cycle [`FilterChain::cycles_until_due`] reported as due.
    pub fn consume(&mut self, sample: f32) {
        // `while` rather than `if` only for an output rate high enough to make the intermediate
        // period shorter than a CPU cycle, which no supported rate is.
        while self.period_counter <= 0 {
            self.period_counter += self.cycles_per_sample;
            self.resample(sample * self.input_gain);
        }
        self.period_counter -= Self::ONE_CYCLE;
    }

    /// Run the intermediate-rate stages, each consuming the one above it.
    fn resample(&mut self, sample: f32) {
        self.high_pass_90.consume(sample);
        self.high_pass_440.consume(self.high_pass_90.output());
        self.fir.consume(self.high_pass_440.output());
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

    /// A signal with the shape of mixed APU output: a few steps, held for many CPU cycles.
    fn stepped(cycle: usize) -> f32 {
        match (cycle / 137) % 4 {
            0 => 0.0,
            1 => 0.121,
            2 => 0.087,
            _ => 0.243,
        }
    }

    /// [`FilterChain::cycles_until_due`] is what lets the mixer skip ~19 CPU cycles in 20, so it
    /// must never step over a cycle on which the chain was due a sample. Walking a cycle at a time
    /// is the definition it has to match.
    #[test]
    fn skipping_to_the_due_cycle_matches_walking_every_cycle() {
        let mut walked = FilterChain::new(NesRegion::Ntsc, 44_100.0);
        let mut skipped = FilterChain::new(NesRegion::Ntsc, 44_100.0);

        let mut expected = Vec::new();
        for cycle in 0..50_000 {
            if walked.cycles_until_due() == 0 {
                walked.consume(stepped(cycle));
            } else {
                walked.skip(1);
            }
            expected.push(walked.output());
        }

        let mut actual = Vec::new();
        let mut cycle = 0;
        while cycle < 50_000 {
            let step = match skipped.cycles_until_due() {
                0 => {
                    skipped.consume(stepped(cycle));
                    1
                }
                due => {
                    let step = due.min(50_000 - cycle);
                    skipped.skip(step);
                    step
                }
            };
            // The output is constant across a skipped span, which is the property that makes the
            // skip sound: record it once per cycle so the two sequences line up.
            actual.resize(actual.len() + step, skipped.output());
            cycle += step;
        }

        assert_eq!(actual.len(), expected.len());
        for (cycle, (a, b)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(a, b, "diverged at cycle {cycle}");
        }
    }

    /// At a 44.1 kHz output rate the intermediate rate is ~88.2 kHz, so the chain wants a sample
    /// about every 20 CPU cycles. If this ever approached 1 the mixer would be back to per-cycle
    /// work, and `Apu::process_outputs`' skip would be buying nothing.
    #[test]
    fn the_chain_samples_about_once_every_twenty_cycles() {
        let mut chain = FilterChain::new(NesRegion::Ntsc, 44_100.0);
        let mut due = 0;
        for cycle in 0..100_000 {
            if chain.cycles_until_due() == 0 {
                chain.consume(stepped(cycle));
                due += 1;
            } else {
                chain.skip(1);
            }
        }
        let cycles_per_sample = 100_000.0 / f64::from(due);
        assert!(
            (20.0..21.0).contains(&cycles_per_sample),
            "{cycles_per_sample} CPU cycles per intermediate sample"
        );
    }

    /// The two `high_pass` stages are the only recursive filters left in the chain, so a DC offset
    /// has to be theirs to remove. This is what distinguishes them from the low-pass stages that
    /// turned out to be constant gains and were folded into `input_gain`.
    #[test]
    fn the_high_pass_stages_reject_dc() {
        let mut chain = FilterChain::new(NesRegion::Ntsc, 44_100.0);
        for _ in 0..200_000 {
            if chain.cycles_until_due() == 0 {
                chain.consume(0.25);
            } else {
                chain.skip(1);
            }
        }
        assert!(
            chain.output().abs() < 1e-4,
            "constant input must decay to silence, got {}",
            chain.output()
        );
    }

    /// Denormals in this chain are a performance cliff, not a rounding detail: the FIR multiplies
    /// all 161 of its taps on every output sample, so once the ring holds denormals *every*
    /// multiply takes an x86 microcode assist. Measured at 49M assists and ~4 billion cycles in a
    /// 1440-frame run - more than the entire cost of mixing - which is why [`flush`] exists.
    ///
    /// Silence is what gets there: the 90 Hz high-pass decays about 0.1% a sample, so it takes
    /// thousands of samples to fall from full scale to zero, and passes through the denormal range
    /// on the way.
    #[test]
    fn decaying_to_silence_leaves_no_denormals() {
        let mut chain = FilterChain::new(NesRegion::Ntsc, 44_100.0);
        // Play something, then go quiet and stay quiet long enough for the tails to decay.
        for cycle in 0..200_000 {
            let sample = if cycle < 40_000 { stepped(cycle) } else { 0.0 };
            if chain.cycles_until_due() == 0 {
                chain.consume(sample);
            } else {
                chain.skip(1);
            }

            let denormal = |value: f32| value != 0.0 && value.abs() < f32::MIN_POSITIVE;
            assert!(
                !chain.fir.inputs.iter().copied().any(denormal),
                "denormal in the FIR ring at cycle {cycle}"
            );
            assert!(
                !denormal(chain.high_pass_90.prev_output)
                    && !denormal(chain.high_pass_90.delta)
                    && !denormal(chain.high_pass_440.prev_output)
                    && !denormal(chain.high_pass_440.delta),
                "denormal in a high-pass stage at cycle {cycle}"
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
