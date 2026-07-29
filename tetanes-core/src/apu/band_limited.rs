//! Band-limited step synthesis for the [`Apu`](crate::apu::Apu).
//!
//! See <https://www.nesdev.org/wiki/APU_Mixer>

// The APU's internal wiring: channel state, dividers and filter taps, whose meaning is the
// hardware's rather than this crate's. Public for embedders and debuggers, not a stable surface -
// see the module docs on `apu`.
#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

/// Synthesises an output-rate waveform from amplitude changes timed in CPU cycles.
///
/// The APU's channels are square, triangle and noise: piecewise constant, changing only when a
/// divider expires. Sampling such a signal at the output rate directly is what aliases - the
/// corners are infinitely wide in frequency, so everything above Nyquist folds back into the
/// audible band, and no filter applied after the fact can separate it out again.
///
/// So rather than sampling, each amplitude change is *added* as a band-limited step: the exact
/// output-rate response of an ideal step at that instant, precomputed at
/// [`BandLimited::PHASES`] sub-sample positions so the timing is kept to a fraction of a sample.
/// Summing those responses reconstructs the waveform with nothing above Nyquist in it.
///
/// The cost follows the number of changes rather than the number of cycles, which for a pulse at
/// a typical period is a few thousand a frame against nearly thirty thousand.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct BandLimited {
    /// Change in output level at each output sample, integrated by [`BandLimited::read`].
    ///
    /// Deltas rather than levels because that is what a step response adds to: writing one step
    /// touches [`BandLimited::WIDTH`] entries instead of every entry from here to the end.
    deltas: Box<[f32]>,
    /// [`BandLimited::PHASES`] impulse responses of [`BandLimited::WIDTH`] taps, one per
    /// sub-sample position.
    kernel: Box<[f32]>,
    /// Output samples per input cycle, in [`BandLimited::FRAC_BITS`] fixed point.
    samples_per_cycle: i64,
    /// Running level, carried across reads so a waveform is continuous over block boundaries.
    level: f32,
}

impl BandLimited {
    /// Sub-sample positions a step can be placed at.
    ///
    /// A step landing on the wrong side of a sample boundary is a timing error of up to half a
    /// sample - at 44.1 kHz, 11 microseconds, which on a square wave's edge is audible as jitter.
    /// 32 phases put it under a third of a microsecond.
    pub const PHASES: usize = 32;
    /// Taps each step response spans, centred on the step.
    ///
    /// A truncated sinc rings; the window suppresses that, and 16 taps is where the remaining
    /// ripple falls below the 16-bit noise floor the output is heading for anyway.
    pub const WIDTH: usize = 16;
    /// Fractional bits in `samples_per_cycle` and in the positions derived from it.
    const FRAC_BITS: u32 = 20;
    const ONE: i64 = 1 << Self::FRAC_BITS;

    /// Create a synthesiser mapping `clock_rate` input cycles onto `sample_rate` output samples,
    /// with room for `capacity` output samples before a [`BandLimited::read`] is required.
    pub fn new(clock_rate: f32, sample_rate: f32, capacity: usize) -> Self {
        Self {
            // `WIDTH` of slack: a step placed at the last sample still writes its whole response.
            deltas: vec![0.0; capacity + Self::WIDTH].into(),
            kernel: Self::build_kernel(),
            samples_per_cycle: ((f64::from(sample_rate) / f64::from(clock_rate)) * Self::ONE as f64)
                as i64,
            level: 0.0,
        }
    }

    /// One windowed-sinc impulse response per sub-sample phase.
    ///
    /// Each phase is normalised to sum to exactly 1 so that a step of amplitude `a` integrates to
    /// `a` whatever fraction of a sample it lands on. Without that the gain would ripple with the
    /// step's sub-sample position, which on a square wave is a buzz at the waveform's own
    /// frequency.
    fn build_kernel() -> Box<[f32]> {
        let mut kernel = vec![0.0f32; Self::PHASES * Self::WIDTH];
        let centre = (Self::WIDTH / 2) as f32;
        for phase in 0..Self::PHASES {
            let offset = phase as f32 / Self::PHASES as f32;
            let taps = &mut kernel[phase * Self::WIDTH..(phase + 1) * Self::WIDTH];
            for (tap, value) in taps.iter_mut().enumerate() {
                // Distance from the step, in output samples.
                let x = tap as f32 - centre + 1.0 - offset;
                // Cutoff at Nyquist, so `sinc(x)` with the sample period as its zero crossing.
                let sinc = if x.abs() < 1e-6 {
                    1.0
                } else {
                    let pi_x = std::f32::consts::PI * x;
                    pi_x.sin() / pi_x
                };
                // Blackman, over the whole span rather than per tap, so the two ends reach zero.
                let w = (tap as f32 + 1.0 - offset) / Self::WIDTH as f32;
                let window = 0.42 - 0.5 * (TAU * w).cos() + 0.08 * (2.0 * TAU * w).cos();
                *value = sinc * window;
            }
            let sum: f32 = taps.iter().sum();
            for value in taps {
                *value /= sum;
            }
        }
        kernel.into()
    }

    /// Number of output samples the block up to `cycle` will produce.
    #[must_use]
    pub const fn samples_at(&self, cycle: u32) -> usize {
        ((cycle as i64 * self.samples_per_cycle) >> Self::FRAC_BITS) as usize
    }

    /// Record a change of `amplitude` in the mixed output, `cycle` input cycles into the block.
    pub fn add_delta(&mut self, cycle: u32, amplitude: f32) {
        if amplitude == 0.0 {
            return;
        }
        let position = cycle as i64 * self.samples_per_cycle;
        let sample = (position >> Self::FRAC_BITS) as usize;
        // The fractional part chooses the response; anything finer than a phase is below the
        // resolution the table has, so it is truncated rather than rounded toward a neighbour.
        let phase = ((position >> (Self::FRAC_BITS - Self::PHASES.ilog2()))
            & (Self::PHASES as i64 - 1)) as usize;

        let taps = &self.kernel[phase * Self::WIDTH..(phase + 1) * Self::WIDTH];
        let Some(deltas) = self.deltas.get_mut(sample..sample + Self::WIDTH) else {
            return;
        };
        for (delta, tap) in deltas.iter_mut().zip(taps) {
            *delta += amplitude * tap;
        }
    }

    /// Integrate the recorded steps into `out`, consuming `count` output samples.
    ///
    /// Whatever a step contributed past `count` stays for the next call, so a waveform crossing a
    /// block boundary is not cut in half.
    pub fn read(&mut self, count: usize, out: &mut Vec<f32>) {
        let count = count.min(self.deltas.len());
        out.reserve(count);
        for delta in &self.deltas[..count] {
            self.level += delta;
            out.push(self.level);
        }
        self.deltas.copy_within(count.., 0);
        let kept = self.deltas.len() - count;
        self.deltas[kept..].fill(0.0);
    }

    /// Drop everything recorded, keeping the running level.
    pub fn clear(&mut self) {
        self.deltas.fill(0.0);
    }

    /// The level the last read left, which is what a new delta is relative to.
    #[must_use]
    pub const fn level(&self) -> f32 {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: f32 = 1_789_772.7;
    const RATE: f32 = 44_100.0;

    fn synth() -> BandLimited {
        BandLimited::new(CLOCK, RATE, 4096)
    }

    /// Magnitude of `samples` at `hz`, by direct transform of a single bin.
    ///
    /// Hann-windowed, which matters more than it looks: measuring an alias means measuring a
    /// small component next to a very large one, and an unwindowed transform's sidelobes fall off
    /// so slowly that the fundamental leaks into the bin being measured and swamps it.
    fn magnitude_at(samples: &[f32], hz: f32) -> f32 {
        let n = samples.len() as f64;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        let mut window_sum = 0.0f64;
        for (i, sample) in samples.iter().enumerate() {
            let window = 0.5 - 0.5 * (f64::from(TAU) * i as f64 / n).cos();
            let phase = f64::from(TAU) * f64::from(hz) * i as f64 / f64::from(RATE);
            re += f64::from(*sample) * window * phase.cos();
            im -= f64::from(*sample) * window * phase.sin();
            window_sum += window;
        }
        (re.hypot(im) / window_sum) as f32
    }

    /// A step of amplitude `a` has to settle at `a`, whatever fraction of an output sample it
    /// lands on. Any dependence on sub-sample position is gain rippling at the waveform's own
    /// frequency, which is exactly the artifact this is meant to remove.
    #[test]
    fn a_step_settles_at_its_amplitude_from_every_sub_sample_position() {
        // Cycles chosen to land all over the sample grid, including on it.
        for cycle in [0, 1, 7, 20, 21, 40, 41, 99, 100, 101, 202, 303] {
            let mut synth = synth();
            synth.add_delta(cycle, 0.25);
            let mut out = Vec::new();
            synth.read(256, &mut out);
            let settled = out[out.len() - 1];
            assert!(
                (settled - 0.25).abs() < 1e-4,
                "step at cycle {cycle} settled at {settled}"
            );
        }
    }

    /// Steps accumulate, so a waveform's level is the sum of everything before it and the reads
    /// that carry it must not lose any of that.
    #[test]
    fn levels_carry_across_reads() {
        let mut synth = synth();
        let mut out = Vec::new();
        for step in 0..8 {
            synth.add_delta(500 + step * 400, 0.1);
            synth.read(200, &mut out);
        }
        // Long enough after the last step for its response to have settled.
        synth.add_delta(4000, 0.0);
        synth.read(400, &mut out);
        let settled = out[out.len() - 1];
        assert!(
            (settled - 0.8).abs() < 1e-3,
            "eight steps of 0.1 settled at {settled}"
        );
    }

    /// The whole point. A square wave's harmonics run past Nyquist; point-sampling folds them
    /// back into the audible band as tones that are not in the signal, and no filter applied
    /// afterwards can remove them because by then they are indistinguishable from real content.
    ///
    /// Here a 5 kHz square is synthesised both ways and measured at the frequency its 13th
    /// harmonic aliases to. Band-limited synthesis has to leave that bin empty.
    #[test]
    fn harmonics_past_nyquist_do_not_fold_back() {
        const FUNDAMENTAL: f32 = 5_000.0;
        // The 13th harmonic is at 65 kHz, which folds to |2*22050 - 65000| = 20.9 kHz.
        let alias = (2.0 * RATE / 2.0 - 13.0 * FUNDAMENTAL).abs();
        let half_period = (CLOCK / FUNDAMENTAL / 2.0) as u32;
        let cycles = half_period * 200;

        let mut synth = BandLimited::new(CLOCK, RATE, 16_384);
        let mut band_limited = Vec::new();
        let mut sampled = Vec::new();
        let mut level = 0.0f32;
        let mut next_edge = 0;

        // Point-sample the same square on the same grid, which is what sampling channel levels at
        // the output rate does.
        let mut next_sample_cycle = 0.0f32;
        let cycles_per_sample = CLOCK / RATE;
        for cycle in 0..cycles {
            if cycle == next_edge {
                let amplitude = if level > 0.0 { -0.5 } else { 0.5 };
                level += amplitude;
                synth.add_delta(cycle, amplitude);
                next_edge += half_period;
            }
            if cycle as f32 >= next_sample_cycle {
                sampled.push(level);
                next_sample_cycle += cycles_per_sample;
            }
        }
        synth.read(sampled.len(), &mut band_limited);

        // Both must have the fundamental, or the comparison is meaningless.
        // Measured against each signal's own fundamental, so this says nothing about gain.
        let tone = magnitude_at(&band_limited, FUNDAMENTAL);
        let sampled_tone = magnitude_at(&sampled, FUNDAMENTAL);
        assert!(tone > 0.05, "the fundamental is missing: {tone}");

        let aliased = magnitude_at(&sampled, alias) / sampled_tone;
        let clean = magnitude_at(&band_limited, alias) / tone;
        assert!(
            clean < aliased / 10.0,
            "alias at {alias:.0} Hz is {clean:.6} of the fundamental, \
             against {aliased:.6} when point-sampled"
        );
    }

    /// A step must land where it was timed to, not merely somewhere near. Sub-sample placement is
    /// what `PHASES` buys, so a step half a sample later has to move the waveform half a sample.
    #[test]
    fn sub_sample_timing_is_kept() {
        let cycles_per_sample = CLOCK / RATE;
        let mut centroids = Vec::new();
        for fraction in [0.0, 0.25, 0.5, 0.75] {
            let mut synth = synth();
            synth.add_delta(
                (100.0 * cycles_per_sample + fraction * cycles_per_sample) as u32,
                1.0,
            );
            let mut out = Vec::new();
            synth.read(256, &mut out);
            // Where the step's energy sits: the centre of mass of the sample-to-sample change.
            let mut weighted = 0.0f64;
            let mut total = 0.0f64;
            for i in 1..out.len() {
                let delta = f64::from(out[i] - out[i - 1]);
                weighted += delta * i as f64;
                total += delta;
            }
            centroids.push(weighted / total);
        }
        for pair in centroids.windows(2) {
            let moved = pair[1] - pair[0];
            assert!(
                (0.2..0.3).contains(&moved),
                "a quarter-sample delay moved the step by {moved} samples: {centroids:?}"
            );
        }
    }
}
