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
                    // The mask is free - the table is a power of two and the PPU only emits
                    // six colour bits and three of emphasis - and it keeps the bounds check out of
                    // the per-pixel loop.
                    *dst = self.palette[usize::from(entry) & (Self::PALETTE_LEN - 1)];
                }
            }
            VideoFilter::Ntsc => {
                deck.frame_buffer_into(self.rgba.as_array_mut());
                self.pack_rgba();
            }
        }
        &self.xrgb
    }

    /// Packs the core's RGBA into XRGB, which is where red and blue change places.
    ///
    /// Split out because that swap is the one thing on this path that can be silently backwards -
    /// a frame in the wrong channel order still looks like a picture.
    fn pack_rgba(&mut self) {
        for (dst, rgba) in self
            .xrgb
            .iter_mut()
            .zip(self.rgba.as_slice().as_chunks::<4>().0)
        {
            *dst = (u32::from(rgba[0]) << 16) | (u32::from(rgba[1]) << 8) | u32::from(rgba[2]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Red and blue change places on the `Ntsc` path, and a frame in the wrong channel order
    /// still looks like a picture - so the swap is asserted against a colour that cannot be
    /// mistaken for its own mirror.
    #[test]
    fn the_ntsc_path_puts_red_and_blue_where_xrgb_wants_them() {
        let mut video = Video::new();
        // A pixel that is unambiguous either way round: mostly red, no blue.
        let rgba = video.rgba.as_array_mut();
        rgba[0] = 0xC0; // R
        rgba[1] = 0x30; // G
        rgba[2] = 0x00; // B
        rgba[3] = 0xFF; // A, which XRGB drops
        video.pack_rgba();

        assert_eq!(
            video.xrgb[0], 0x00C0_3000,
            "0x00RRGGBB, so red is the high byte and the alpha is gone"
        );
    }

    /// The `Pixellate` path is the same conversion done as a table, so it has to agree about which
    /// byte is which - a table built the other way round would tint every frame.
    #[test]
    fn the_palette_table_is_built_in_the_same_channel_order() {
        let video = Video::new();
        for entry in [0, 1, 0x16, 0x2A, Video::PALETTE_LEN - 1] {
            let rgb = &Ppu::NTSC_PALETTE[entry * 3..];
            assert_eq!(
                video.palette[entry],
                (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]),
                "palette entry {entry}"
            );
            assert_eq!(
                video.palette[entry] & 0xFF00_0000,
                0,
                "the X byte stays clear"
            );
        }
    }

    /// Every index the PPU can emit has to land inside the table, or the mask in `frame` would be
    /// hiding an out-of-range entry rather than saving a bounds check.
    #[test]
    fn the_palette_covers_every_index_the_ppu_emits() {
        assert_eq!(
            Video::PALETTE_LEN,
            512,
            "64 colours by eight emphasis states"
        );
        assert!(Ppu::NTSC_PALETTE.len() >= Video::PALETTE_LEN * 3);
    }

    #[test]
    fn a_frame_is_the_pitch_the_frontend_is_told() {
        assert_eq!(PITCH, 256 * 4);
        assert_eq!(PIXELS * 4, PITCH * usize::from(size::HEIGHT));
    }
}
