use super::{Sprite, RENDER_CHANNELS, RENDER_HEIGHT, RENDER_SIZE, RENDER_WIDTH};
use crate::common::Powered;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::{f32::consts::PI, fmt};

lazy_static! {
    static ref NTSC_PALETTE: Vec<Vec<Vec<u32>>> = {
        // NOTE: There's lot's to clean up here -- too many magic numbers and duplication but
        // I'm afraid to touch it now that it works
        // Source: https://bisqwit.iki.fi/jutut/kuvat/programming_examples/nesemu1/nesemu1.cc
        // http://wiki.nesdev.com/w/index.php/NTSC_video

        // Calculate the luma and chroma by emulating the relevant circuits:
        const VOLTAGES: [i32; 16] = [
            -6, -69, 26, -59, 29, -55, 73, -40, 68, -17, 125, 11, 68, 33, 125, 78,
        ];

        let mut ntsc_palette = vec![vec![vec![0; 512]; 64]; 3];

        // Helper functions for converting YIQ to RGB
        let gammafix = |color: f32| {
            if color < 0.0 {
                0.0
            } else {
                color.powf(2.2 / 1.8)
            }
        };
        let clamp = |color| {
            if color > 255.0 {
                255
            } else {
                color as u32
            }
        };
        let yiq_divider = (9 * 10u32.pow(6)) as f32;
        for (palette_offset, palette) in ntsc_palette.iter_mut().enumerate() {
            for channel in 0..3 {
                for color0_offset in 0..512 {
                    let emphasis = color0_offset / 64;

                    for (color1_offset, color1) in palette.iter_mut().enumerate() {
                        let mut y = 0;
                        let mut i = 0;
                        let mut q = 0;
                        // 12 samples of NTSC signal constitute a color.
                        for sample in 0..12 {
                            let noise = (sample + palette_offset * 4) % 12;
                            // Sample either the previous or the current pixel.
                            // Use pixel=color0 to disable artifacts.
                            let pixel = if noise < 5 - channel * 2 {
                                color0_offset
                            } else {
                                color1_offset
                            };

                            // Decode the color index.
                            let chroma = pixel % 16;
                            let luma = if chroma < 0xE { (pixel / 4) & 12 } else { 4 }; // Forces luma to 0, 4, 8, or 12 for easy lookup
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
                            let (sin, cos) = (PI * sample as f32 / 6.0).sin_cos();
                            y += level;
                            i += level * (cos * 5909.0) as i32;
                            q += level * (sin * 5909.0) as i32;
                        }
                        // Store color at subpixel precision
                        let y = y as f32 / 1980.0;
                        let i = i as f32;
                        let q = q as f32;
                        match channel {
                            2 => {
                                let rgb = y + i * 0.947 / yiq_divider + q * 0.624 / yiq_divider;
                                color1[color0_offset] +=
                                    0x10000 * clamp(255.0 * gammafix(rgb));
                            }
                            1 => {
                                let rgb = y + i * -0.275 / yiq_divider + q * -0.636 / yiq_divider;
                                color1[color0_offset] +=
                                    0x00100 * clamp(255.0 * gammafix(rgb));
                            }
                            0 => {
                                let rgb = y + i * -1.109 / yiq_divider + q * 1.709 / yiq_divider;
                                color1[color0_offset] +=
                                    clamp(255.0 * gammafix(rgb));
                            }
                            _ => (), // invalid channel
                        }
                    }
                }
            }
        }

        ntsc_palette
    };
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Frame {
    pub num: u32,
    pub parity: bool,
    // Shift registers
    pub tile_lo: u8,
    pub tile_hi: u8,
    // Tile data - stored in cycles 0 mod 8
    pub nametable: u16,
    pub attribute: u8,
    pub tile_data: u64,
    // Sprite data
    pub sprite_count: u8,
    pub sprite_zero_on_line: bool,
    pub sprites: [Sprite; 8], // Each frame can only hold 8 sprites at a time
    pub prev_pixel: u32,
    pub pixels: Vec<u8>,
}

impl Frame {
    pub(super) fn new() -> Self {
        Self {
            num: 0,
            parity: false,
            nametable: 0,
            attribute: 0,
            tile_lo: 0,
            tile_hi: 0,
            tile_data: 0,
            sprite_count: 0,
            sprite_zero_on_line: false,
            sprites: [Sprite::new(); 8],
            prev_pixel: 0xFFFF,
            pixels: vec![0; RENDER_SIZE],
        }
    }

    pub(super) fn increment(&mut self) {
        self.num += 1;
        self.parity = !self.parity;
    }

    pub(super) fn put_pixel(&mut self, x: u32, y: u32, red: u8, green: u8, blue: u8) {
        if x >= RENDER_WIDTH || y >= RENDER_HEIGHT {
            return;
        }
        let idx = RENDER_CHANNELS * (x + y * RENDER_WIDTH) as usize;
        self.pixels[idx] = red;
        self.pixels[idx + 1] = green;
        self.pixels[idx + 2] = blue;
    }

    // Amazing implementation Bisqwit! Much faster than my original, but boy what a pain
    // to translate it to Rust
    // Source: https://bisqwit.iki.fi/jutut/kuvat/programming_examples/nesemu1/nesemu1.cc
    // http://wiki.nesdev.com/w/index.php/NTSC_video
    //
    // Note: Because blending relies on previous x pixel, we shift everything to the
    // left and render an extra pixel column on the right
    pub(super) fn put_ntsc_pixel(&mut self, x: u32, y: u32, mut pixel: u32, ppu_cycle: u32) {
        if x > RENDER_WIDTH || y >= RENDER_HEIGHT {
            return;
        }
        if x == RENDER_WIDTH {
            pixel = self.prev_pixel;
        }
        let color =
            NTSC_PALETTE[ppu_cycle as usize][(self.prev_pixel % 64) as usize][pixel as usize];
        self.prev_pixel = pixel;
        let red = (color >> 16 & 0xFF) as u8;
        let green = (color >> 8 & 0xFF) as u8;
        let blue = (color & 0xFF) as u8;
        self.put_pixel(x.saturating_sub(1), y, red, green, blue);
    }
}

impl Powered for Frame {
    fn reset(&mut self) {
        self.num = 0;
        self.parity = false;
    }
    fn power_cycle(&mut self) {
        self.reset();
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("num", &self.num)
            .field("parity", &self.parity)
            .field("tile_lo", &format_args!("${:02X}", &self.tile_lo))
            .field("tile_hi", &format_args!("${:02X}", &self.tile_hi))
            .field("nametable", &format_args!("${:04X}", &self.nametable))
            .field("attribute", &format_args!("${:02X}", &self.attribute))
            .field("tile_data", &format_args!("${:16X}", &self.tile_data))
            .field("sprite_count", &self.sprite_count)
            .field("sprite_zero_on_line", &self.sprite_zero_on_line)
            .field("sprites", &self.sprites)
            .field("prev_pixel", &self.prev_pixel)
            .field("pixels", &self.pixels)
            .finish()
    }
}
