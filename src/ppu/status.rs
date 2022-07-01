use crate::common::{Kind, Reset};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    // $2002 PPUSTATUS
    //
    // http://wiki.nesdev.com/w/index.php/PPU_registers#PPUSTATUS
    // VSO. ....
    // |||+-++++- PPU open bus. Returns stale PPU bus contents.
    // ||+------- Sprite overflow. The intent was for this flag to be set
    // ||         whenever more than eight sprites appear on a scanline, but a
    // ||         hardware bug causes the actual behavior to be more complicated
    // ||         and generate false positives as well as false negatives; see
    // ||         PPU sprite evaluation. This flag is set during sprite
    // ||         evaluation and cleared at dot 1 (the second dot) of the
    // ||         pre-render line.
    // |+-------- Sprite 0 Hit.  Set when a nonzero pixel of sprite 0 overlaps
    // |          a nonzero background pixel; cleared at dot 1 of the pre-render
    // |          line.  Used for raster timing.
    // +--------- Vertical blank has started (0: not in vblank; 1: in vblank)
    //            Set at dot 1 of line 241 (the line *after* the post-render
    //            line); cleared after reading $2002 and at dot 1 of the
    //            pre-render line.
    #[derive(Default, Serialize, Deserialize)]
    #[must_use]
    pub struct PpuStatus: u8 {
        const UNUSED1 = 0x01;
        const UNUSED2 = 0x02;
        const UNUSED3 = 0x04;
        const UNUSED4 = 0x08;
        const UNUSED5 = 0x10;
        const SPR_OVERFLOW = 0x20;
        const SPR_ZERO_HIT = 0x40;
        const VBLANK_STARTED = 0x80;
    }
}

impl PpuStatus {
    pub const fn new() -> Self {
        Self::from_bits_truncate(0x00)
    }

    #[inline]
    pub fn write(&mut self, val: u8) {
        self.bits = val;
    }

    #[inline]
    #[must_use]
    pub const fn read(&self) -> u8 {
        self.bits
    }

    #[inline]
    pub fn set_spr_overflow(&mut self, val: bool) {
        self.set(Self::SPR_OVERFLOW, val);
    }

    #[inline]
    #[must_use]
    pub const fn spr_zero_hit(&self) -> bool {
        self.contains(Self::SPR_ZERO_HIT)
    }

    #[inline]
    pub fn set_spr_zero_hit(&mut self, val: bool) {
        self.set(Self::SPR_ZERO_HIT, val);
    }

    #[inline]
    #[must_use]
    pub const fn in_vblank(&self) -> bool {
        self.contains(Self::VBLANK_STARTED)
    }

    #[inline]
    pub fn set_in_vblank(&mut self, val: bool) {
        self.set(Self::VBLANK_STARTED, val);
    }

    #[inline]
    pub fn reset_in_vblank(&mut self) {
        self.remove(Self::VBLANK_STARTED);
    }
}

impl Reset for PpuStatus {
    // https://www.nesdev.org/wiki/PPU_power_up_state
    fn reset(&mut self, kind: Kind) {
        if kind == Kind::Hard {
            self.set_in_vblank(false); // Technically random
            self.set_spr_zero_hit(false);
            self.set_spr_overflow(false); // Technically random
        }
    }
}
