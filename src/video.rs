use crate::ppu::Ppu;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
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

impl From<usize> for VideoFilter {
    fn from(value: usize) -> Self {
        if value == 1 {
            Self::Ntsc
        } else {
            Self::Pixellate
        }
    }
}

#[derive(Clone)]
#[must_use]
pub struct Video {
    filter: VideoFilter,
    output: Vec<u8>,
}

impl Default for Video {
    fn default() -> Self {
        Self::new()
    }
}

impl Video {
    pub fn new() -> Self {
        let mut output = vec![0x00; 4 * Ppu::SIZE];
        // Force alpha to 255.
        for p in output.iter_mut().skip(3).step_by(4) {
            *p = 255;
        }
        Self {
            filter: VideoFilter::default(),
            output,
        }
    }

    #[inline]
    pub const fn filter(&self) -> VideoFilter {
        self.filter
    }

    #[inline]
    pub fn set_filter(&mut self, filter: VideoFilter) {
        self.filter = filter;
    }

    // Returns a fully rendered frame of RENDER_SIZE RGB colors
    pub fn apply_filter(&mut self, buffer: &[u16], frame_number: u32) {
        match self.filter {
            VideoFilter::Pixellate => self.decode_buffer(buffer),
            VideoFilter::Ntsc => self.apply_ntsc_filter(buffer, frame_number),
        }
    }

    #[inline]
    #[must_use]
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn decode_buffer(&mut self, buffer: &[u16]) {
        assert!(buffer.len() * 4 == self.output.len());
        for (pixel, colors) in buffer.iter().zip(self.output.chunks_exact_mut(4)) {
            assert!(colors.len() > 2);
            let (red, green, blue) = Ppu::system_palette(*pixel);
            colors[0] = red;
            colors[1] = green;
            colors[2] = blue;
            // Alpha should always be 255
        }
    }

    // Amazing implementation Bisqwit! Much faster than my original, but boy what a pain
    // to translate it to Rust
    // Source: https://bisqwit.iki.fi/jutut/kuvat/programming_examples/nesemu1/nesemu1.cc
    // http://wiki.nesdev.com/w/index.php/NTSC_video
    pub fn apply_ntsc_filter(&mut self, buffer: &[u16], frame_number: u32) {
        assert!(buffer.len() * 4 == self.output.len());
        let mut prev_pixel = 0;
        for (idx, (pixel, colors)) in buffer
            .iter()
            .zip(self.output.chunks_exact_mut(4))
            .enumerate()
        {
            let x = idx % 256;
            let color = if x == 0 {
                // Remove pixel 0 artifact from not having a valid previous pixel
                0
            } else {
                let y = idx / 256;
                let even_phase = if frame_number & 0x01 == 0x01 { 0 } else { 1 };
                let phase = (2 + y * 341 + x + even_phase) % 3;
                NTSC_PALETTE
                    [phase + ((prev_pixel & 0x3F) as usize) * 3 + (*pixel as usize) * 3 * 64]
            };
            prev_pixel = u32::from(*pixel);
            assert!(colors.len() > 2);
            colors[0] = (color >> 16 & 0xFF) as u8;
            colors[1] = (color >> 8 & 0xFF) as u8;
            colors[2] = (color & 0xFF) as u8;
            // Alpha should always be 255
        }
    }
}

impl std::fmt::Debug for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Video")
            .field("filter", &self.filter)
            .field("output_len", &self.output.len())
            .finish()
    }
}

pub static NTSC_PALETTE: Lazy<Vec<u32>> = Lazy::new(|| {
    // NOTE: There's lot's to clean up here -- too many magic numbers and duplication but
    // I'm afraid to touch it now that it works
    // Source: https://bisqwit.iki.fi/jutut/kuvat/programming_examples/nesemu1/nesemu1.cc
    // http://wiki.nesdev.com/w/index.php/NTSC_video

    // Calculate the luma and chroma by emulating the relevant circuits:
    const VOLTAGES: [i32; 16] = [
        -6, -69, 26, -59, 29, -55, 73, -40, 68, -17, 125, 11, 68, 33, 125, 78,
    ];

    let mut ntsc_palette = vec![0; 512 * 64 * 3];

    // Helper functions for converting YIQ to RGB
    let gamma = 2.0; // Assumed display gamma
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
                        // Use pixel=color0 to disable artifacts.
                        let pixel = if noise < 6 - channel * 2 {
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
                                255.95 * gammafix(q.mul_add(0.623_557, i.mul_add(0.946_882, y)));
                            ntsc_palette[idx] += 0x10000 * rgb.clamp(0.0, 255.0) as u32;
                        }
                        1 => {
                            let rgb =
                                255.95 * gammafix(q.mul_add(-0.635_691, i.mul_add(-0.274_788, y)));
                            ntsc_palette[idx] += 0x00100 * rgb.clamp(0.0, 255.0) as u32;
                        }
                        0 => {
                            let rgb =
                                255.95 * gammafix(q.mul_add(1.709_007, i.mul_add(-1.108_545, y)));
                            ntsc_palette[idx] += rgb.clamp(0.0, 255.0) as u32;
                        }
                        _ => (), // invalid channel
                    }
                }
            }
        }
    }

    ntsc_palette
});
