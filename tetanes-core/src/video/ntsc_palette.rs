// Generates the NTSC filter's color lookup table.
//
// `include!`d by `build.rs`, which bakes the table into the binary, and by `video.rs`'s tests,
// which assert the baked table is what this produces. It is deliberately free of crate imports
// for that reason - `build.rs` cannot link the crate it is building.
//
// Amazing implementation Bisqwit! Much faster than my original, but boy what a pain to translate
// it to Rust.
//
// Source: <https://bisqwit.iki.fi/jutut/kuvat/programming_examples/nesemu1/nesemu1.cc>
// See also: <https://wiki.nesdev.org/w/index.php/NTSC_video>

/// Number of entries in the table: 512 colors x 64 previous colors x 3 phases.
pub const NTSC_PALETTE_LEN: usize = 512 * 64 * 3;

/// Generate the table as `NTSC_PALETTE_LEN` red, green, blue triples.
///
/// One entry is the color the filter produces for a (phase, previous color, color) triple, which
/// is what makes the lookup a single index rather than a signal decode per pixel.
pub fn generate_ntsc_palette() -> Vec<u8> {
    // NOTE: There's lot's to clean up here -- too many magic numbers and duplication but
    // I'm afraid to touch it now that it works

    // Calculate the luma and chroma by emulating the relevant circuits:
    const VOLTAGES: [i32; 16] = [
        -6, -69, 26, -59, 29, -55, 73, -40, 68, -17, 125, 11, 68, 33, 125, 78,
    ];

    let mut ntsc_palette = vec![0u8; NTSC_PALETTE_LEN * 3];

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
                        let (sin, cos) = (std::f64::consts::PI * sample as f64 / 6.0).sin_cos();
                        y += level;
                        i += level * (cos * 5909.0) as i32;
                        q += level * (sin * 5909.0) as i32;
                    }
                    // Store color at subpixel precision
                    let y = f64::from(y) / 1980.0;
                    let i = f64::from(i) / yiq_divider;
                    let q = f64::from(q) / yiq_divider;
                    let idx = palette_offset + color0_offset * 3 * 64 + color1_offset * 3;
                    // Each channel is a separate pass over the whole table, so this writes one
                    // byte of the triple and leaves the other two for the passes either side.
                    match channel {
                        2 => {
                            let rgb =
                                255.0 * gammafix(q.mul_add(0.623_557, i.mul_add(0.946_882, y)));
                            ntsc_palette[idx * 3] = rgb.clamp(0.0, 255.0) as u8;
                        }
                        1 => {
                            let rgb =
                                255.0 * gammafix(q.mul_add(-0.635_691, i.mul_add(-0.274_788, y)));
                            ntsc_palette[idx * 3 + 1] = rgb.clamp(0.0, 255.0) as u8;
                        }
                        0 => {
                            let rgb =
                                255.0 * gammafix(q.mul_add(1.709_007, i.mul_add(-1.108_545, y)));
                            ntsc_palette[idx * 3 + 2] = rgb.clamp(0.0, 255.0) as u8;
                        }
                        _ => (), // invalid channel
                    }
                }
            }
        }
    }

    ntsc_palette
}
