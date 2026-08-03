//! PPUMASK register implementation.
//!
//! See: <https://wiki.nesdev.org/w/index.php/PPU_registers#PPUMASK>

// The PPU's internal register and fetch state, whose meaning is the hardware's rather than this
// crate's. Public for embedders and debuggers, not a stable surface - see the module docs on `ppu`.
#![allow(missing_docs)]

use crate::common::{NesRegion, ResetKind};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// PPUMASK register.
///
/// See: <https://wiki.nesdev.org/w/index.php/PPU_registers#PPUMASK>
#[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
#[must_use]
pub struct Mask {
    pub emphasis: u16,
    pub grayscale: u8,
    pub rendering_enabled: bool,
    pub prev_rendering_enabled: bool,
    pub pending_rendering_update: bool,
    /// First dot on which the background is drawn, or [`Mask::NEVER_DRAWN`] when it is not.
    ///
    /// Collapses "is the background on, and is this dot past the left-column clip" into one
    /// comparison for the pixel path, which asks it 61,440 times a frame.
    // Derived from `bits`; recomputed after a state load rather than stored, so the save format
    // does not depend on it.
    #[serde(skip)]
    pub min_draw_bg_cycle: u16,
    /// First dot on which sprites are drawn. See [`Mask::min_draw_bg_cycle`].
    #[serde(skip)]
    pub min_draw_spr_cycle: u16,
    pub bits: Bits,
    pub region: NesRegion,
}

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

impl Mask {
    pub fn new(region: NesRegion) -> Self {
        let mut mask = Self {
            region,
            ..Default::default()
        };
        mask.write(0);
        mask
    }

    #[inline]
    pub fn write(&mut self, val: u8) {
        self.bits = Bits::from_bits_truncate(val);
        self.grayscale = if self.bits.contains(Bits::GRAYSCALE) {
            0x30
        } else {
            0x3F
        };
        self.pending_rendering_update =
            self.rendering_enabled != (self.show_bg() || self.show_spr());
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
        self.bits.contains(Bits::SHOW_BG)
    }

    /// Whether sprites are shown at all.
    #[inline]
    pub const fn show_spr(&self) -> bool {
        self.bits.contains(Bits::SHOW_SPR)
    }

    /// Whether the background is shown in the leftmost 8 pixels.
    #[inline]
    pub const fn show_left_bg(&self) -> bool {
        self.bits.contains(Bits::SHOW_LEFT_BG)
    }

    /// Whether sprites are shown in the leftmost 8 pixels.
    #[inline]
    pub const fn show_left_spr(&self) -> bool {
        self.bits.contains(Bits::SHOW_LEFT_SPR)
    }

    /// Recompute the dot thresholds the pixel path compares against.
    ///
    /// Public because they are derived rather than stored: a state restored from disk carries
    /// `bits` and the flags but not these, and [`Bus::load_state`](crate::bus::Bus::load_state)
    /// puts them back.
    pub const fn update_draw_thresholds(&mut self) {
        // The left-column clips hide the first 8 pixels, i.e. everything up to and including dot 8.
        self.min_draw_bg_cycle = if self.show_bg() {
            if self.show_left_bg() { 0 } else { 8 }
        } else {
            Self::NEVER_DRAWN
        };
        self.min_draw_spr_cycle = if self.show_spr() {
            if self.show_left_spr() { 0 } else { 8 }
        } else {
            Self::NEVER_DRAWN
        };
    }

    pub fn update_emphasis(&mut self) {
        self.emphasis = u16::from(
            match self.region {
                NesRegion::Auto | NesRegion::Ntsc => self.bits.intersection(
                    Bits::EMPHASIZE_RED | Bits::EMPHASIZE_GREEN | Bits::EMPHASIZE_BLUE,
                ),
                NesRegion::Pal | NesRegion::Dendy => {
                    // Red/Green are swapped for PAL/Dendy
                    let mut emphasis = self.bits.intersection(Bits::EMPHASIZE_BLUE);
                    emphasis.set(
                        Bits::EMPHASIZE_GREEN,
                        self.bits.contains(Bits::EMPHASIZE_RED),
                    );
                    emphasis.set(
                        Bits::EMPHASIZE_RED,
                        self.bits.contains(Bits::EMPHASIZE_GREEN),
                    );
                    emphasis
                }
            }
            .bits(),
        ) << 1;
    }

    #[inline]
    pub fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.update_emphasis();
    }
    // https://www.nesdev.org/wiki/PPU_power_up_state
    pub fn reset(&mut self, _kind: ResetKind) {
        self.write(0);
    }
    pub const fn clock(&mut self) {
        // Rendering enabled flag is set with a 1 cycle delay (setting it at cycle N won't take
        // effect until cycle N+2)
        if self.pending_rendering_update {
            self.pending_rendering_update = false;

            self.prev_rendering_enabled = self.rendering_enabled;
            self.rendering_enabled = self.show_bg() || self.show_spr();
            self.pending_rendering_update = self.prev_rendering_enabled != self.rendering_enabled;
        }
    }
}
