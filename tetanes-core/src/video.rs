//! Video output and filtering.

use crate::ppu::{self, Ppu};
use serde::{Deserialize, Serialize};
use std::{
    ops::{Index, IndexMut},
    slice::SliceIndex,
};
use thiserror::Error;

/// Error parsing a [`VideoFilter`] from a string.
#[derive(Error, Debug)]
#[must_use]
#[error("failed to parse `VideoFilter`")]
pub struct ParseVideoFilterError;

/// How a raw PPU frame is turned into RGBA pixels.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum VideoFilter {
    /// One RGBA pixel per PPU pixel, straight from the palette.
    Pixellate,
    /// An NTSC composite simulation, which is what a CRT would have shown.
    #[default]
    Ntsc,
}

impl VideoFilter {
    /// Every filter, for enumerating them in a UI.
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

/// One frame of RGBA output.
///
/// The size is part of the type: a `Frame` is always exactly [`Frame::SIZE`] bytes and there is no
/// way to resize one, so [`Frame::as_array`] hands out a fixed-size array without a fallible
/// conversion at every call.
#[derive(Debug, Clone)]
#[must_use]
pub struct Frame(Box<[u8; Frame::SIZE]>);

impl Frame {
    /// Size of a frame in bytes: one RGBA pixel per [`ppu::size::FRAME`] pixel.
    pub const SIZE: usize = ppu::size::FRAME * 4;

    /// Allocate a new frame for video output, opaque black.
    ///
    /// # Panics
    ///
    /// Never: the `Vec` is allocated at exactly [`Frame::SIZE`] two lines above the conversion. It
    /// is built as a `Vec` rather than with `Box::new([0; SIZE])` because the latter materialises
    /// all 240 KiB as a stack temporary before moving it to the heap, which overflows the stack in
    /// an unoptimized build.
    pub fn new() -> Self {
        let mut pixels = vec![0; Self::SIZE];
        // Load-bearing: the filters write only the RGB bytes of each pixel, so alpha is set once
        // here and never again. Leaving it zeroed renders every frame fully transparent.
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = u8::MAX;
        }
        Self(
            pixels
                .into_boxed_slice()
                .try_into()
                .expect("`Frame::SIZE` bytes were just allocated"),
        )
    }

    /// The frame's length in bytes, which is always [`Frame::SIZE`].
    // A frame is a fixed size and never empty, so there is nothing for `is_empty` to report.
    #[allow(clippy::len_without_is_empty)]
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        Self::SIZE
    }

    /// Borrows the frame's pixels.
    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn as_slice(&self) -> &[u8] {
        &*self.0
    }

    /// Mutably borrows the frame's pixels.
    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut *self.0
    }

    /// Borrows the frame's pixels as a fixed-size array.
    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn as_array(&self) -> &[u8; Self::SIZE] {
        &self.0
    }

    /// Mutably borrows the frame's pixels as a fixed-size array.
    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn as_array_mut(&mut self) -> &mut [u8; Self::SIZE] {
        &mut self.0
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl AsRef<[u8]> for Frame {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for Frame {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

// Indexing, but not `Deref<Target = Vec<u8>>`: slicing a frame is useful - the renderer trims
// overscan with it - while `push`/`clear`/`truncate` would break the size the type promises.
impl<I: SliceIndex<[u8]>> Index<I> for Frame {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        Index::index(self.as_slice(), index)
    }
}

impl<I: SliceIndex<[u8]>> IndexMut<I> for Frame {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        IndexMut::index_mut(self.as_mut_slice(), index)
    }
}

/// Turns raw PPU frames into RGBA output.
#[derive(Clone)]
#[must_use]
pub struct Video {
    /// The filter applied to each frame.
    pub filter: VideoFilter,
    /// The frame the last [`Video::apply_filter`] wrote into.
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

    /// Applies the configured filter to a raw PPU frame and returns the RGBA result.
    pub fn apply_filter(
        &mut self,
        buffer: &[u16; ppu::size::FRAME],
        frame_number: u32,
    ) -> &[u8; Frame::SIZE] {
        let output = self.frame.as_array_mut();
        match self.filter {
            VideoFilter::Pixellate => Self::decode_buffer(buffer, output),
            VideoFilter::Ntsc => Self::apply_ntsc_filter(buffer, frame_number, output),
        }

        self.frame.as_array()
    }

    /// Applies the configured filter to a raw PPU frame, writing the RGBA result into `output`.
    pub fn apply_filter_into(
        &self,
        buffer: &[u16; ppu::size::FRAME],
        frame_number: u32,
        output: &mut [u8; Frame::SIZE],
    ) {
        match self.filter {
            VideoFilter::Pixellate => Self::decode_buffer(buffer, output),
            VideoFilter::Ntsc => Self::apply_ntsc_filter(buffer, frame_number, output),
        }
    }

    /// Fills a fully rendered frame with RGB colors.
    pub fn decode_buffer(buffer: &[u16; ppu::size::FRAME], output: &mut [u8; Frame::SIZE]) {
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
    pub fn apply_ntsc_filter(
        buffer: &[u16; ppu::size::FRAME],
        frame_number: u32,
        output: &mut [u8; Frame::SIZE],
    ) {
        // Hoisted out of the per-pixel loop: `even_phase` only depends on the frame parity, not
        // on the pixel.
        let even_phase = u32::from(frame_number & 0x01 != 0x01);

        let mut prev_color = 0;
        // Rolling replacement for the per-pixel `(2 + y * 341 + x + even_phase) % 3`: `phase`
        // only ever needs `+ 1 mod 3` per pixel, recomputed from scratch just once per row.
        let mut phase = 0;
        for (idx, (color, pixels)) in buffer.iter().zip(output.chunks_exact_mut(4)).enumerate() {
            let x = idx % 256;
            let entry = if x == 0 {
                // Remove pixel 0 artifact from not having a valid previous pixel
                let y = (idx / 256) as u32;
                phase = (2 + y * 341 + even_phase) % 3;
                None
            } else {
                phase = if phase == 2 { 0 } else { phase + 1 };
                let index = phase as usize
                    + ((prev_color & 0x3F) as usize) * 3
                    + (*color as usize) * 3 * 64;
                NTSC_PALETTE.get(index * 3..index * 3 + 3)
            };
            prev_color = u32::from(*color);
            // Alpha is left alone: `Frame::new` sets it to 255 and nothing here clears it.
            match entry {
                Some(rgb) => pixels[..3].copy_from_slice(rgb),
                None => pixels[..3].fill(0),
            }
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

/// The NTSC filter's lookup table: one red, green, blue triple per (phase, previous color, color).
///
/// Computed by `build.rs` from `src/video/ntsc_palette.rs` and baked in, rather than generated on
/// first use.
// It depends on nothing but constants, and the ~30 ms of `powf` and `sin_cos` it takes would
// otherwise land on whichever frame first turns the filter on. The length - 512 colors x 64 previous
// colors x 3 phases, as triples - is spelled out rather than imported from the generator, which
// `build.rs` owns; `ntsc_palette_matches_the_generator` asserts the two agree byte for byte.
pub static NTSC_PALETTE: &[u8; 512 * 64 * 3 * 3] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ntsc_palette.bin"));

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame is opaque black at power-on, and the filters only ever write the RGB bytes - so an
    /// alpha this constructor failed to set would never be set at all, and every frame would
    /// render fully transparent.
    #[test]
    fn a_new_frame_is_opaque_black() {
        let frame = Frame::new();
        assert_eq!(frame.len(), Frame::SIZE);
        assert_eq!(frame.as_array().len(), Frame::SIZE);

        for (i, pixel) in frame.as_slice().chunks_exact(4).enumerate() {
            assert_eq!(pixel, [0, 0, 0, u8::MAX], "pixel {i}");
        }
    }

    /// The overscan trim the renderer does, which is what `Index` exists for.
    #[test]
    fn a_frame_can_be_sliced_but_not_resized() {
        let mut frame = Frame::new();
        let trim = 4 * usize::from(ppu::size::WIDTH) * 8;

        assert_eq!(
            frame[trim..frame.len() - trim].len(),
            Frame::SIZE - 2 * trim
        );
        assert_eq!(frame[..].len(), Frame::SIZE);

        frame[0] = 0x12;
        assert_eq!(frame.as_slice()[0], 0x12);
        assert_eq!(frame.len(), Frame::SIZE, "still the same size");
    }

    /// Reference form of `apply_ntsc_filter`'s color lookup, computing `phase` directly from
    /// `(2 + y * 341 + x + even_phase) % 3` each pixel rather than the rolling counter. Guards
    /// that hoisting `even_phase`/`get_or_init` and replacing the per-pixel `%3` with a rolling
    /// counter didn't change output.
    fn apply_ntsc_filter_reference(buffer: &[u16], frame_number: u32, output: &mut [u8]) {
        let mut prev_color = 0;
        for (idx, (color, pixels)) in buffer.iter().zip(output.chunks_exact_mut(4)).enumerate() {
            let x = idx % 256;
            let rgb = if x == 0 {
                [0, 0, 0]
            } else {
                let y = idx / 256;
                let even_phase = if frame_number & 0x01 == 0x01 { 0 } else { 1 };
                let phase = (2 + y * 341 + x + even_phase) % 3;
                let index = phase + ((prev_color & 0x3F) as usize) * 3 + (*color as usize) * 3 * 64;
                [
                    NTSC_PALETTE[index * 3],
                    NTSC_PALETTE[index * 3 + 1],
                    NTSC_PALETTE[index * 3 + 2],
                ]
            };
            prev_color = u32::from(*color);
            pixels[..3].copy_from_slice(&rgb);
        }
    }

    /// The generator `build.rs` runs, so the baked table can be held to what it produces. The
    /// crate itself never runs it - that is the whole point of baking the table.
    mod generator {
        include!("video/ntsc_palette.rs");
    }

    /// The table shipped in the binary has to be the one `src/video/ntsc_palette.rs` describes:
    /// nothing else in the suite would notice `build.rs` writing a stale or truncated file, since
    /// every other test compares the filter against itself.
    #[test]
    fn ntsc_palette_matches_the_generator() {
        let expected = generator::generate_ntsc_palette();

        assert_eq!(
            NTSC_PALETTE.len(),
            generator::NTSC_PALETTE_LEN * 3,
            "baked table is not `NTSC_PALETTE_LEN` triples"
        );
        assert_eq!(NTSC_PALETTE.len(), expected.len());
        for (i, (actual, expected)) in NTSC_PALETTE.iter().zip(&expected).enumerate() {
            assert_eq!(
                actual,
                expected,
                "byte {i} (entry {}, channel {})",
                i / 3,
                i % 3
            );
        }
    }

    #[test]
    fn ntsc_filter_matches_reference_formula() {
        // A synthetic full-frame buffer exercising every palette index, not a real capture.
        let mut buffer = Box::new([0u16; ppu::size::FRAME]);
        for (i, px) in buffer.iter_mut().enumerate() {
            *px = (i % 64) as u16;
        }
        let mut actual = Box::new([0u8; Frame::SIZE]);
        let mut expected = vec![0u8; Frame::SIZE];

        for frame_number in [0u32, 1, 2, 3, 100, 101] {
            Video::apply_ntsc_filter(&buffer, frame_number, &mut actual);
            apply_ntsc_filter_reference(&buffer[..], frame_number, &mut expected);
            assert_eq!(
                &actual[..],
                &expected[..],
                "mismatch at frame_number={frame_number}"
            );
        }
    }
}
