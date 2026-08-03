//! Digital filters for the [`Apu`](crate::apu::Apu).
//!
//! See <https://www.nesdev.org/wiki/APU_Mixer>

// The APU's internal wiring: channel state, dividers and filter taps, whose meaning is the
// hardware's rather than this crate's. Public for embedders and debuggers, not a stable surface -
// see the module docs on `apu`.
#![allow(missing_docs)]

use crate::common::NesRegion;
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

/// Flushes a value too small to hear down to zero.
///
/// The recursive stages here decay exponentially toward silence at ~0.1% a sample, so after every
/// note they spend thousands of samples in the denormal range. On x86 each denormal operand costs
/// a microcode assist of ~100 cycles, and they are contagious - one filter's tail becomes the
/// next's input - so this is a performance cliff rather than a rounding nicety: measured at 49M
/// assists and ~4 billion cycles over a 1440-frame run.
///
/// `1e-20` is ~400 dB below full scale, so nothing audible is discarded, and it is far above the
/// denormal boundary (`1.2e-38`) so no denormal is ever produced.
#[inline(always)]
fn flush(value: f32) -> f32 {
    if value.abs() < 1e-20 { 0.0 } else { value }
}

/// A one-pole infinite impulse response (IIR) low-pass filter.
///
/// `y[n] = y[n-1] + alpha * (x[n] - y[n-1])`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct LowPass {
    pub alpha: f32,
    pub output: f32,
}

impl LowPass {
    pub fn new(sample_rate: f32, cutoff: f32) -> Self {
        let period = 1.0 / sample_rate;
        let rc = 1.0 / (TAU * cutoff);
        Self {
            alpha: period / (rc + period),
            output: 0.0,
        }
    }

    pub fn consume(&mut self, sample: f32) -> f32 {
        self.output = flush(self.output + self.alpha * (sample - self.output));
        self.output
    }
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

/// The filters applied to synthesised output, at the output rate.
///
/// These are the console's own RC corners - two high-pass stages and a 14 kHz low-pass - applied
/// in that order, and nothing else. Keeping the output below Nyquist is
/// [`BandLimited`](crate::apu::band_limited::BandLimited)'s job, and it does it by construction
/// rather than by filtering afterwards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterChain {
    pub region: NesRegion,
    /// First-order high-pass at 90 Hz.
    pub high_pass_90: HighPass,
    /// First-order high-pass at 440 Hz.
    pub high_pass_440: HighPass,
    /// First-order low-pass at 14 kHz.
    pub low_pass_14k: LowPass,
}

impl FilterChain {
    /// Overall output level.
    ///
    /// The mixer tables reach about 1.0 between them, so this leaves better than 6 dB of headroom
    /// for the high-pass stages' transient overshoot and for the ringing either side of a
    /// band-limited step. Its value is the level the emulator has always output rather than
    /// anything derived - a console has no absolute output level to be right about, and changing
    /// how loud it is is not this change's business.
    pub const OUTPUT_GAIN: f32 = 0.4715;

    pub fn new(region: NesRegion, output_rate: f32) -> Self {
        // TODO: Support famicom filter selection
        // // first-order high-pass filter at 37 Hz
        // HighPass::new(output_rate, 37.0),
        Self {
            region,
            high_pass_90: HighPass::new(output_rate, 90.0),
            high_pass_440: HighPass::new(output_rate, 440.0),
            low_pass_14k: LowPass::new(output_rate, 14_000.0),
        }
    }

    /// Filter a run of output-rate samples in place.
    pub fn filter(&mut self, samples: &mut [f32]) {
        for sample in samples {
            self.high_pass_90.consume(*sample * Self::OUTPUT_GAIN);
            self.high_pass_440.consume(self.high_pass_90.output());
            *sample = self.low_pass_14k.consume(self.high_pass_440.output());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two high-pass stages are the only recursive filters here, so a DC offset is theirs to
    /// remove - and removing it is what they are for, since the mixer tables are all positive and
    /// the console's output rides on a large one.
    #[test]
    fn the_high_pass_stages_reject_dc() {
        let mut chain = FilterChain::new(NesRegion::Ntsc, 44_100.0);
        let mut samples = [0.25f32; 200_000];
        chain.filter(&mut samples);
        let settled = samples[samples.len() - 1];
        assert!(
            settled.abs() < 1e-4,
            "constant input must decay to silence, got {settled}"
        );
    }

    /// Denormals in a decaying filter are a performance cliff on x86, each one costing a microcode
    /// assist, and the high-pass tails run down through that range after every note.
    #[test]
    fn decaying_to_silence_leaves_no_denormals() {
        let mut chain = FilterChain::new(NesRegion::Ntsc, 44_100.0);
        let mut samples = [0.25f32; 4_000];
        chain.filter(&mut samples);
        let mut silence = [0.0f32; 200_000];
        chain.filter(&mut silence);

        let denormal = |value: f32| value != 0.0 && value.abs() < f32::MIN_POSITIVE;
        assert!(
            !denormal(chain.high_pass_90.prev_output)
                && !denormal(chain.high_pass_90.delta)
                && !denormal(chain.high_pass_440.prev_output)
                && !denormal(chain.high_pass_440.delta),
            "denormal left in a high-pass stage"
        );
        assert!(
            !silence.iter().copied().any(denormal),
            "denormal in the output"
        );
    }
}
