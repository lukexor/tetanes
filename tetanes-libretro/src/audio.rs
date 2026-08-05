//! Turning the APU's output into what the frontend wants to hear.
//!
//! The core produces mono `f32`; libretro takes interleaved stereo `i16`. The NES is mono, so both
//! channels get the same sample.

/// Sample rate the core runs at and the frontend is told about.
///
/// Matches `Apu::DEFAULT_SAMPLE_RATE`, which is already what audio devices and frontends run at, so
/// nothing downstream has to resample.
pub const SAMPLE_RATE: f64 = 48_000.0;

/// The interleaved buffer handed to the frontend, reused across frames.
#[derive(Default)]
pub struct Audio {
    stereo: Vec<i16>,
}

impl Audio {
    /// Interleaves one frame of mono samples, returning `(pointer, frames)` for the batch callback.
    ///
    /// Empty when the frame produced no audio, which is what a headless or muted deck does.
    pub fn interleave(&mut self, mono: &[f32]) -> &[i16] {
        self.stereo.clear();
        self.stereo.reserve(mono.len() * 2);
        for &sample in mono {
            // Clamped rather than wrapped: the filter chain can overshoot slightly, and an `as`
            // cast on an out-of-range float saturates in Rust but a wrap would sound like a click.
            let scaled = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
            self.stereo.push(scaled);
            self.stereo.push(scaled);
        }
        &self.stereo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mono_sample_becomes_a_stereo_pair() {
        let mut audio = Audio::default();
        let stereo = audio.interleave(&[0.0, 0.5, -0.5]);
        assert_eq!(stereo.len(), 6);
        assert_eq!(stereo[0], stereo[1]);
        assert_eq!(stereo[2], stereo[3]);
        assert!(stereo[2] > 0 && stereo[4] < 0);
    }

    #[test]
    fn a_sample_past_full_scale_clamps_rather_than_wrapping() {
        let mut audio = Audio::default();
        let stereo = audio.interleave(&[2.0, -2.0]);
        assert_eq!(stereo[0], i16::MAX);
        assert_eq!(stereo[2], -i16::MAX);
    }

    /// The buffer is reused, so a short frame after a long one must not leave the tail behind.
    #[test]
    fn a_frame_does_not_inherit_the_last_one() {
        let mut audio = Audio::default();
        assert_eq!(audio.interleave(&[0.1; 8]).len(), 16);
        assert_eq!(audio.interleave(&[0.1; 2]).len(), 4);
        assert!(audio.interleave(&[]).is_empty());
    }
}
