//! PPUSTATUS register implementation.
//!
//! See: <https://wiki.nesdev.com/w/index.php/PPU_registers#PPUSTATUS>

use crate::common::{Reset, ResetKind};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// PPUSTATUS register.
///
/// See: <https://wiki.nesdev.com/w/index.php/PPU_registers#PPUSTATUS>
#[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
#[must_use]
pub struct Status {
    pub spr_overflow: bool,
    pub spr_zero_hit: bool,
    pub in_vblank: bool,
    bits: Bits,
}

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
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
    #[must_use]
    pub struct Bits: u8 {
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

impl Status {
    pub fn new() -> Self {
        let mut status = Self::default();
        status.write(0);
        status
    }

    pub const fn write(&mut self, val: u8) {
        self.bits = Bits::from_bits_truncate(val);
        self.spr_overflow = self.bits.contains(Bits::SPR_ZERO_HIT);
        self.spr_zero_hit = self.bits.contains(Bits::SPR_ZERO_HIT);
        self.in_vblank = self.bits.contains(Bits::VBLANK_STARTED);
    }

    #[must_use]
    pub const fn read(&self) -> u8 {
        self.bits.bits()
    }

    pub fn set_spr_overflow(&mut self, val: bool) {
        self.bits.set(Bits::SPR_OVERFLOW, val);
        self.spr_overflow = val;
    }

    pub fn set_spr_zero_hit(&mut self, val: bool) {
        self.bits.set(Bits::SPR_ZERO_HIT, val);
        self.spr_zero_hit = val;
    }

    pub fn set_in_vblank(&mut self, val: bool) {
        self.bits.set(Bits::VBLANK_STARTED, val);
        self.in_vblank = val;
    }

    pub fn reset_in_vblank(&mut self) {
        self.bits.remove(Bits::VBLANK_STARTED);
        self.in_vblank = false;
    }
}

impl Reset for Status {
    // https://www.nesdev.org/wiki/PPU_power_up_state
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            self.set_in_vblank(false); // Technically random
            self.set_spr_zero_hit(false);
            self.set_spr_overflow(false); // Technically random
        }
    }
}
