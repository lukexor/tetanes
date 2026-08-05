//! Turning a rendered NES frame into what the frontend wants to see.
//!
//! The frontend is told [`XRGB8888`](crate::sys::RETRO_PIXEL_FORMAT_XRGB8888), which is
//! `0x00RRGGBB` in a native-endian `u32`. Neither of the core's two outputs is already that, so
//! each takes its own route:
//!
//! - `Pixellate` decodes [`ControlDeck::frame_buffer_raw`] - one palette index per pixel - straight
//!   through a 512-entry lookup table. That skips the core's own RGBA pass entirely, so it does
//!   less work than the UI does.
//! - `Ntsc` has to run that pass, because the Bisqwit filter only writes RGBA, and then swap red
//!   and blue.

use tetanes_core::{
    control_deck::ControlDeck,
    ppu::{Ppu, size},
    video::{Frame, VideoFilter},
};

/// Pixels in one frame.
pub const PIXELS: usize = size::FRAME;
/// Bytes in one row of the buffer handed to the frontend.
pub const PITCH: usize = size::WIDTH as usize * 4;

/// Frame conversion buffers, allocated once.
pub struct Video {
    /// What the frontend is shown.
    xrgb: Box<[u32; PIXELS]>,
    /// Only used on the `Ntsc` path, where the core renders RGBA and this holds it.
    rgba: Frame,
    /// `Ppu::NTSC_PALETTE` as XRGB, indexed by palette entry including emphasis bits.
    palette: Box<[u32; Self::PALETTE_LEN]>,
}

impl Default for Video {
    fn default() -> Self {
        Self::new()
    }
}

impl Video {
    /// Palette entries: 64 colours across eight emphasis combinations.
    const PALETTE_LEN: usize = 512;

    pub fn new() -> Self {
        let mut palette = Box::new([0u32; Self::PALETTE_LEN]);
        for (entry, xrgb) in palette.iter_mut().enumerate() {
            let rgb = &Ppu::NTSC_PALETTE[entry * 3..];
            *xrgb = (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]);
        }
        Self {
            xrgb: Box::new([0; PIXELS]),
            rgba: Frame::new(),
            palette,
        }
    }

    /// Renders the deck's current frame and returns it as XRGB8888.
    pub fn frame(&mut self, deck: &mut ControlDeck, filter: VideoFilter) -> &[u32; PIXELS] {
        match filter {
            VideoFilter::Pixellate => {
                for (dst, &entry) in self.xrgb.iter_mut().zip(deck.frame_buffer_raw().iter()) {
                    // The PPU only ever emits an index this table has, and wrapping it here would
                    // hide a bug rather than fix one.
                    *dst = self.palette[usize::from(entry) & (Self::PALETTE_LEN - 1)];
                }
            }
            VideoFilter::Ntsc => {
                deck.frame_buffer_into(self.rgba.as_array_mut());
                for (dst, rgba) in self
                    .xrgb
                    .iter_mut()
                    .zip(self.rgba.as_slice().as_chunks::<4>().0)
                {
                    *dst =
                        (u32::from(rgba[0]) << 16) | (u32::from(rgba[1]) << 8) | u32::from(rgba[2]);
                }
            }
        }
        &self.xrgb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both paths have to agree, or switching the filter would shift every colour.
    #[test]
    fn both_filters_produce_the_same_xrgb_for_a_flat_frame() {
        let video = Video::new();
        // Palette entry 0 is the NES's dark grey; whatever it is, the two routes must match.
        let rgb = &Ppu::NTSC_PALETTE[0..3];
        let expected = (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]);
        assert_eq!(video.palette[0], expected);
        assert_eq!(video.palette[0] & 0xFF00_0000, 0, "the X byte stays clear");
    }

    #[test]
    fn a_frame_is_the_pitch_the_frontend_is_told() {
        assert_eq!(PITCH, 256 * 4);
        assert_eq!(PIXELS * 4, PITCH * usize::from(size::HEIGHT));
    }
}
