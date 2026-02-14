//! Video output and filtering.

use crate::ppu::Ppu;
use serde::{Deserialize, Serialize};
use std::{
    f64::consts::PI,
    ops::{Deref, DerefMut},
    sync::OnceLock,
};
use thiserror::Error;

#[derive(Error, Debug)]
#[must_use]
#[error("failed to parse `VideoFilter`")]
pub struct ParseVideoFilterError;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum VideoFilter {
    Pixellate,
    #[default]
    Ntsc,
}

impl VideoFilter {
    pub const fn as_slice() -> &'static [Self] {
        &[Self::Pixellate, Self::Ntsc]
    }
}

impl AsRef<str> for VideoFilter {
    fn as_ref(&self) -> &str {
        match self {
            Self::Pixellate => "Pixellate",
            Self::Ntsc => "NTSC",
        }
    }
}

impl TryFrom<usize> for VideoFilter {
    type Error = ParseVideoFilterError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Pixellate,
            1 => Self::Ntsc,
            _ => return Err(ParseVideoFilterError),
        })
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct Frame(Vec<u8>);

impl Frame {
    pub const SIZE: usize = Ppu::SIZE * 4;

    /// Allocate a new frame for video output.
    pub fn new() -> Self {
        Self(
            [(); Self::SIZE / 4]
                .into_iter()
                .flat_map(|_| [0, 0, 0, 255])
                .collect(),
        )
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Frame {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Frame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone)]
#[must_use]
pub struct Video {
    pub filter: VideoFilter,
    pub frame: Frame,
}

impl Default for Video {
    fn default() -> Self {
        Self::new()
    }
}

impl Video {
    /// Create a new Video decoder with the default filter.
    pub fn new() -> Self {
        Self::with_filter(VideoFilter::default())
    }

    /// Create a new Video encoder with a filter.
    pub fn with_filter(filter: VideoFilter) -> Self {
        Self {
            filter,
            frame: Frame::new(),
        }
    }

    /// Applies the given filter to the given video buffer and returns the result.
    pub fn apply_filter(&mut self, buffer: &[u16], frame_number: u32) -> &[u8] {
        match self.filter {
            VideoFilter::Pixellate => Self::decode_buffer(buffer, &mut self.frame),
            VideoFilter::Ntsc => Self::apply_ntsc_filter(buffer, frame_number, &mut self.frame),
        }

        &self.frame
    }

    /// Applies the given filter to the given video buffer by coping into the provided buffer.
    pub fn apply_filter_into(&self, buffer: &[u16], frame_number: u32, output: &mut [u8]) {
        match self.filter {
            VideoFilter::Pixellate => Self::decode_buffer(buffer, output),
            VideoFilter::Ntsc => Self::apply_ntsc_filter(buffer, frame_number, output),
        }
    }

    /// Fills a fully rendered frame with RGB colors.
    pub fn decode_buffer(buffer: &[u16], output: &mut [u8]) {
        for (color, pixels) in buffer.iter().zip(output.chunks_exact_mut(4)) {
            let index = (*color as usize) * 3;
            assert!(Ppu::NTSC_PALETTE.len() > index + 2);
            assert!(pixels.len() > 2);
            pixels[0] = Ppu::NTSC_PALETTE[index];
            pixels[1] = Ppu::NTSC_PALETTE[index + 1];
            pixels[2] = Ppu::NTSC_PALETTE[index + 2];
        }
    }

    /// Applies the NTSC filter to the given video buffer.
    ///
    /// Amazing implementation Bisqwit! Much faster than my original, but boy what a pain
    /// to translate it to Rust
    /// Source: <https://bisqwit.iki.fi/jutut/kuvat/programming_examples/nesemu1/nesemu1.cc>
    /// See also: <https://wiki.nesdev.org/w/index.php/NTSC_video>
    pub fn apply_ntsc_filter(buffer: &[u16], frame_number: u32, output: &mut [u8]) {
        let mut prev_color = 0;
        for (idx, (color, pixels)) in buffer.iter().zip(output.chunks_exact_mut(4)).enumerate() {
            let x = idx % 256;
            let rgba = if x == 0 {
                // Remove pixel 0 artifact from not having a valid previous pixel
                0
            } else {
                let y = idx / 256;
                let even_phase = if frame_number & 0x01 == 0x01 { 0 } else { 1 };
                let phase = (2 + y * 341 + x + even_phase) % 3;
                NTSC_PALETTE.get_or_init(generate_ntsc_palette)
                    [phase + ((prev_color & 0x3F) as usize) * 3 + (*color as usize) * 3 * 64]
            };
            prev_color = u32::from(*color);
            assert!(pixels.len() > 2);
            pixels[0] = ((rgba >> 16) & 0xFF) as u8;
            pixels[1] = ((rgba >> 8) & 0xFF) as u8;
            pixels[2] = (rgba & 0xFF) as u8;
            // Alpha should always be 255
        }
    }
}

impl std::fmt::Debug for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Video")
            .field("filter", &self.filter)
            .finish()
    }
}

pub static NTSC_PALETTE: OnceLock<Vec<u32>> = OnceLock::new();
fn generate_ntsc_palette() -> Vec<u32> {
    // NOTE: There's lot's to clean up here -- too many magic numbers and duplication but
    // I'm afraid to touch it now that it works
    // Source: https://bisqwit.iki.fi/jutut/kuvat/programming_examples/nesemu1/nesemu1.cc
    // https://wiki.nesdev.org/w/index.php/NTSC_video

    // Calculate the luma and chroma by emulating the relevant circuits:
    const VOLTAGES: [i32; 16] = [
        -6, -69, 26, -59, 29, -55, 73, -40, 68, -17, 125, 11, 68, 33, 125, 78,
    ];

    let mut ntsc_palette = vec![0; 512 * 64 * 3];

    // Helper functions for converting YIQ to RGB
    let gamma = 1.8; // Assumed display gamma
    let gammafix = |color: f64| {
        if color <= 0.0 {
            0.0
        } else {
            color.powf(2.2 / gamma)
        }
    };
    let yiq_divider = f64::from(9 * 10u32.pow(6));
    for palette_offset in 0..3 {
        for channel in 0..3 {
            for color0_offset in 0..512 {
                let emphasis = color0_offset / 64;

                for color1_offset in 0..64 {
                    let mut y = 0;
                    let mut i = 0;
                    let mut q = 0;
                    // 12 samples of NTSC signal constitute a color.
                    for sample in 0..12 {
                        let noise = (sample + palette_offset * 4) % 12;
                        // Sample either the previous or the current pixel.
                        // Use pixel=color0_offset to disable artifacts.
                        let pixel = if noise < 5 - channel * 2 {
                            color0_offset
                        } else {
                            color1_offset
                        };

                        // Decode the color index.
                        let chroma = pixel & 0x0F;
                        // Forces luma to 0, 4, 8, or 12 for easy lookup
                        let luma = if chroma < 0x0E { (pixel / 4) & 12 } else { 4 };
                        // NES NTSC modulator (square wave between up to four voltage levels):
                        let limit = if (chroma + 8 + sample) % 12 < 6 {
                            12
                        } else {
                            0
                        };
                        let high = if chroma > limit { 1 } else { 0 };
                        let emp_effect = if (152_278 >> (sample / 2 * 3)) & emphasis > 0 {
                            0
                        } else {
                            2
                        };
                        let level = 40 + VOLTAGES[high + emp_effect + luma];
                        // Ideal TV NTSC demodulator:
                        let (sin, cos) = (PI * sample as f64 / 6.0).sin_cos();
                        y += level;
                        i += level * (cos * 5909.0) as i32;
                        q += level * (sin * 5909.0) as i32;
                    }
                    // Store color at subpixel precision
                    let y = f64::from(y) / 1980.0;
                    let i = f64::from(i) / yiq_divider;
                    let q = f64::from(q) / yiq_divider;
                    let idx = palette_offset + color0_offset * 3 * 64 + color1_offset * 3;
                    match channel {
                        2 => {
                            let rgb =
                                255.0 * gammafix(q.mul_add(0.623_557, i.mul_add(0.946_882, y)));
                            ntsc_palette[idx] += 0x10000 * rgb.clamp(0.0, 255.0) as u32;
                        }
                        1 => {
                            let rgb =
                                255.0 * gammafix(q.mul_add(-0.635_691, i.mul_add(-0.274_788, y)));
                            ntsc_palette[idx] += 0x00100 * rgb.clamp(0.0, 255.0) as u32;
                        }
                        0 => {
                            let rgb =
                                255.0 * gammafix(q.mul_add(1.709_007, i.mul_add(-1.108_545, y)));
                            ntsc_palette[idx] += rgb.clamp(0.0, 255.0) as u32;
                        }
                        _ => (), // invalid channel
                    }
                }
            }
        }
    }

    ntsc_palette
}
