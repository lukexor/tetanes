use crate::common::{NesRegion, Reset, ResetKind};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
#[must_use]
pub struct Mask {
    pub rendering_enabled: bool,
    pub grayscale: u16,
    pub emphasis: u16,
    pub show_left_bg: bool,
    pub show_left_spr: bool,
    pub show_bg: bool,
    pub show_spr: bool,
    pub region: NesRegion,
    bits: Bits,
}

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

    pub fn write(&mut self, val: u8) {
        self.bits = Bits::from_bits_truncate(val);
        self.grayscale = if self.bits.contains(Bits::GRAYSCALE) {
            0x30
        } else {
            0x3F
        };
        self.show_left_bg = self.bits.contains(Bits::SHOW_LEFT_BG);
        self.show_left_spr = self.bits.contains(Bits::SHOW_LEFT_SPR);
        self.show_bg = self.bits.contains(Bits::SHOW_BG);
        self.show_spr = self.bits.contains(Bits::SHOW_SPR);
        self.rendering_enabled = self.show_bg || self.show_spr;
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

    pub fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.write(self.bits.bits());
    }
}

impl Reset for Mask {
    // https://www.nesdev.org/wiki/PPU_power_up_state
    fn reset(&mut self, _kind: ResetKind) {
        self.write(0);
    }
}
