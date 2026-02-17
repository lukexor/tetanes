//! PPUMASK register implementation.
//!
//! See: <https://wiki.nesdev.org/w/index.php/PPU_registers#PPUMASK>

use crate::common::{Clock, NesRegion, Reset, ResetKind};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// PPUMASK register.
///
/// See: <https://wiki.nesdev.org/w/index.php/PPU_registers#PPUMASK>
#[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
#[must_use]
#[repr(C)]
pub struct Mask {
    /// Raw mask bits.
    pub bits: Bits,
    /// Rendering enabled is set with a 1 cycle delay (setting it at cycle N won't take effect
    /// until cycle N+2)
    pub delayed_bits: DelayedBits,
    /// Cached as it's checked very often.
    pub rendering_enabled: bool,
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

    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
    #[must_use]
    pub struct DelayedBits: u8 {
        const PREV_RENDERING_ENABLED = 0x01;
        const REQUIRES_UPDATE = 0x02;
    }
}

impl Mask {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn write(&mut self, val: u8) {
        self.bits = Bits::from_bits_truncate(val);
        self.delayed_bits.set(
            DelayedBits::REQUIRES_UPDATE,
            self.rendering_enabled() != self.rendering_enabled_raw(),
        );
    }

    #[inline(always)]
    #[must_use]
    pub const fn grayscale(&self) -> u8 {
        if self.bits.contains(Bits::GRAYSCALE) {
            0x30
        } else {
            0x3F
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn show_left_bg(&self) -> bool {
        self.bits.contains(Bits::SHOW_LEFT_BG)
    }

    #[inline(always)]
    #[must_use]
    pub const fn show_left_spr(&self) -> bool {
        self.bits.contains(Bits::SHOW_LEFT_SPR)
    }

    #[inline(always)]
    #[must_use]
    pub const fn show_bg(&self) -> bool {
        self.bits.contains(Bits::SHOW_BG)
    }

    #[inline(always)]
    #[must_use]
    pub const fn show_spr(&self) -> bool {
        self.bits.contains(Bits::SHOW_SPR)
    }

    #[inline(always)]
    #[must_use]
    pub const fn rendering_enabled_raw(&self) -> bool {
        self.show_bg() || self.show_spr()
    }

    #[inline(always)]
    #[must_use]
    pub const fn prev_rendering_enabled(&self) -> bool {
        self.delayed_bits
            .contains(DelayedBits::PREV_RENDERING_ENABLED)
    }

    #[inline(always)]
    #[must_use]
    pub const fn rendering_enabled(&self) -> bool {
        self.rendering_enabled
    }

    #[inline(always)]
    #[must_use]
    pub fn emphasis(&self, region: NesRegion) -> u16 {
        u16::from(
            match region {
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
        ) << 1
    }
}

impl Clock for Mask {
    fn clock(&mut self) {
        // Rendering enabled flag is set with a 1 cycle delay (setting it at cycle N won't take
        // effect until cycle N+2)
        if self.delayed_bits.contains(DelayedBits::REQUIRES_UPDATE) {
            self.delayed_bits.remove(DelayedBits::REQUIRES_UPDATE);

            let rendering_enabled = self.rendering_enabled();
            self.delayed_bits
                .set(DelayedBits::PREV_RENDERING_ENABLED, rendering_enabled);

            let rendering_enabled_raw = self.rendering_enabled_raw();
            self.rendering_enabled = rendering_enabled_raw;
            self.delayed_bits.set(
                DelayedBits::REQUIRES_UPDATE,
                rendering_enabled != rendering_enabled_raw,
            );
        }
    }
}

impl Reset for Mask {
    // https://www.nesdev.org/wiki/PPU_power_up_state
    fn reset(&mut self, _kind: ResetKind) {
        self.write(0);
    }
}
