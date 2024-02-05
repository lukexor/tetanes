use crate::common::{Kind, NesRegion, Reset};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    // $2001 PPUMASK
    //
    // http://wiki.nesdev.com/w/index.php/PPU_registers#PPUMASK
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
    pub struct PpuMask: u8 {
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

impl PpuMask {
    pub const fn new() -> Self {
        Self::from_bits_truncate(0x00)
    }

    #[inline]
    pub fn write(&mut self, val: u8) {
        *self = Self::from_bits_truncate(val);
    }

    #[inline]
    #[must_use]
    pub const fn grayscale(&self) -> bool {
        self.contains(Self::GRAYSCALE)
    }

    #[inline]
    #[must_use]
    pub const fn show_left_bg(&self) -> bool {
        self.contains(Self::SHOW_LEFT_BG)
    }

    #[inline]
    #[must_use]
    pub const fn show_left_spr(&self) -> bool {
        self.contains(Self::SHOW_LEFT_SPR)
    }

    #[inline]
    #[must_use]
    pub const fn show_bg(&self) -> bool {
        self.contains(Self::SHOW_BG)
    }

    #[inline]
    #[must_use]
    pub const fn show_spr(&self) -> bool {
        self.contains(Self::SHOW_SPR)
    }

    #[inline]
    #[must_use]
    pub fn emphasis(&self, region: NesRegion) -> u8 {
        let emphasis = match region {
            NesRegion::Ntsc => self
                .intersection(Self::EMPHASIZE_RED | Self::EMPHASIZE_GREEN | Self::EMPHASIZE_BLUE),
            NesRegion::Pal | NesRegion::Dendy => {
                // Red/Green are swapped for PAL/Dendy
                let mut emphasis = self.intersection(Self::EMPHASIZE_BLUE);
                emphasis.set(Self::EMPHASIZE_GREEN, self.contains(Self::EMPHASIZE_RED));
                emphasis.set(Self::EMPHASIZE_RED, self.contains(Self::EMPHASIZE_GREEN));
                emphasis
            }
        };
        emphasis.bits()
    }
}

impl Reset for PpuMask {
    // https://www.nesdev.org/wiki/PPU_power_up_state
    fn reset(&mut self, _kind: Kind) {
        *self = Self::empty();
    }
}
