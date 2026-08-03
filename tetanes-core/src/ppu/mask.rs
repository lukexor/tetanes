//! PPUMASK register implementation.
//!
//! See: <https://wiki.nesdev.org/w/index.php/PPU_registers#PPUMASK>
//!
//! The register's fields live on [`Ppu`] as `mask_*` rather than in a struct of their own, so that
//! field order alone decides which of them share the dot loop's cache line - the draw thresholds
//! are wanted per pixel, `mask_emphasis` once a frame. What the register *does* stays here, in an
//! `impl Ppu` block, the same way a CPU-bus access lives with the state it reads.
//!
//! Region lives on [`Ppu::region`]; there is no second copy of it here.

// The PPU's internal register and fetch state, whose meaning is the hardware's rather than this
// crate's. Public for embedders and debuggers, not a stable surface - see the module docs on `ppu`.
#![allow(missing_docs)]

use crate::{
    common::{NesRegion, ResetKind},
    ppu::Ppu,
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    // $2001 PPUMASK
    //
    // https://wiki.nesdev.org/w/index.php/PPU_registers#PPUMASK
    // BGRs bMmG
    // |||| |||+- Grayscale (0: normal color, 1: produce a grayscale display)
    // |||| ||+-- 1: Show background in leftmost 8 pixels of screen, 0: Hide
    // |||| |+--- 1: Show sprites in leftmost 8 pixels of screen, 0: Hide
    // |||| +---- 1: Show background
    // |||+------ 1: Show sprites
    // ||+------- Emphasize red
    // |+-------- Emphasize green
    // +--------- Emphasize blue
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
    #[must_use]
    pub struct Bits: u8 {
        const GRAYSCALE = 0x01;
        const SHOW_LEFT_BG = 0x02;
        const SHOW_LEFT_SPR = 0x04;
        const SHOW_BG = 0x08;
        const SHOW_SPR = 0x10;
        const EMPHASIZE_RED = 0x20;
        const EMPHASIZE_GREEN = 0x40;
        const EMPHASIZE_BLUE = 0x80;
    }
}

impl Ppu {
    #[inline]
    pub fn write_mask(&mut self, val: u8) {
        self.mask_bits = Bits::from_bits_truncate(val);
        self.mask_grayscale = if self.mask_bits.contains(Bits::GRAYSCALE) {
            0x30
        } else {
            0x3F
        };
        self.mask_pending_rendering_update =
            self.mask_rendering_enabled != (self.show_bg() || self.show_spr());
        self.update_draw_thresholds();
        self.update_emphasis();
    }

    /// A dot beyond the end of a scanline, so `cycle > threshold` is never true.
    pub const NEVER_DRAWN: u16 = 300;

    /// Whether the background is shown at all.
    //
    // Decoded from `bits` on demand rather than stored: the dot loop asks
    // [`Mask::min_draw_bg_cycle`] instead, and four bools here are four bytes out of the 64 the
    // per-dot working set has to fit in.
    #[inline]
    pub const fn show_bg(&self) -> bool {
        self.mask_bits.contains(Bits::SHOW_BG)
    }

    /// Whether sprites are shown at all.
    #[inline]
    pub const fn show_spr(&self) -> bool {
        self.mask_bits.contains(Bits::SHOW_SPR)
    }

    /// Whether the background is shown in the leftmost 8 pixels.
    #[inline]
    pub const fn show_left_bg(&self) -> bool {
        self.mask_bits.contains(Bits::SHOW_LEFT_BG)
    }

    /// Whether sprites are shown in the leftmost 8 pixels.
    #[inline]
    pub const fn show_left_spr(&self) -> bool {
        self.mask_bits.contains(Bits::SHOW_LEFT_SPR)
    }

    /// Recompute the dot thresholds the pixel path compares against.
    ///
    /// Public because they are derived rather than stored: a state restored from disk carries
    /// `bits` and the flags but not these, and [`Bus::load_state`](crate::bus::Bus::load_state)
    /// puts them back.
    pub const fn update_draw_thresholds(&mut self) {
        // The left-column clips hide the first 8 pixels, i.e. everything up to and including dot 8.
        self.mask_min_draw_bg_cycle = if self.show_bg() {
            if self.show_left_bg() { 0 } else { 8 }
        } else {
            Ppu::NEVER_DRAWN
        };
        self.mask_min_draw_spr_cycle = if self.show_spr() {
            if self.show_left_spr() { 0 } else { 8 }
        } else {
            Ppu::NEVER_DRAWN
        };
    }

    pub fn update_emphasis(&mut self) {
        self.mask_emphasis = u16::from(
            match self.region {
                NesRegion::Auto | NesRegion::Ntsc => self.mask_bits.intersection(
                    Bits::EMPHASIZE_RED | Bits::EMPHASIZE_GREEN | Bits::EMPHASIZE_BLUE,
                ),
                NesRegion::Pal | NesRegion::Dendy => {
                    // Red/Green are swapped for PAL/Dendy
                    let mut emphasis = self.mask_bits.intersection(Bits::EMPHASIZE_BLUE);
                    emphasis.set(
                        Bits::EMPHASIZE_GREEN,
                        self.mask_bits.contains(Bits::EMPHASIZE_RED),
                    );
                    emphasis.set(
                        Bits::EMPHASIZE_RED,
                        self.mask_bits.contains(Bits::EMPHASIZE_GREEN),
                    );
                    emphasis
                }
            }
            .bits(),
        ) << 1;
    }

    // https://www.nesdev.org/wiki/PPU_power_up_state
    pub fn reset_mask(&mut self, _kind: ResetKind) {
        self.write_mask(0);
    }
    pub const fn clock_mask(&mut self) {
        // Rendering enabled flag is set with a 1 cycle delay (setting it at cycle N won't take
        // effect until cycle N+2)
        if self.mask_pending_rendering_update {
            self.mask_pending_rendering_update = false;

            self.mask_prev_rendering_enabled = self.mask_rendering_enabled;
            self.mask_rendering_enabled = self.show_bg() || self.show_spr();
            self.mask_pending_rendering_update =
                self.mask_prev_rendering_enabled != self.mask_rendering_enabled;
        }
    }
}
