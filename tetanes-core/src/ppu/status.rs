//! PPUSTATUS register implementation.
//!
//! See: <https://wiki.nesdev.org/w/index.php/PPU_registers#PPUSTATUS>
//!
//! The register's fields live on [`Ppu`] as `status_*` rather than in a struct of their own, so
//! that field order alone decides what shares the dot loop's cache line. What the register *does*
//! stays here, in an `impl Ppu` block, the same way a CPU-bus access lives with the state it reads.

// The PPU's internal register and fetch state, whose meaning is the hardware's rather than this
// crate's. Public for embedders and debuggers, not a stable surface - see the module docs on `ppu`.
#![allow(missing_docs)]

use crate::{common::ResetKind, ppu::Ppu};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    // $2002 PPUSTATUS
    //
    // https://wiki.nesdev.org/w/index.php/PPU_registers#PPUSTATUS
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

impl Ppu {
    /// Decode a $2002 PPUSTATUS write into the `status_*` fields.
    #[inline]
    pub const fn write_status(&mut self, val: u8) {
        let bits = Bits::from_bits_truncate(val);
        self.status_spr_overflow = bits.contains(Bits::SPR_ZERO_HIT);
        self.status_spr_zero_hit = bits.contains(Bits::SPR_ZERO_HIT);
        self.status_in_vblank = bits.contains(Bits::VBLANK_STARTED);
    }

    /// The raw $2002 PPUSTATUS bits, without the read side effects.
    //
    // Composed from the three flags rather than kept alongside them: the register has no other
    // readable content - bits 0-4 are open bus, filled in by `Ppu::peek_status` - so a stored copy
    // would only be a second place to update, one of them inside the pixel loop.
    #[inline(always)]
    #[must_use]
    pub const fn read_status_bits(&self) -> u8 {
        ((self.status_in_vblank as u8) << 7)
            | ((self.status_spr_zero_hit as u8) << 6)
            | ((self.status_spr_overflow as u8) << 5)
    }

    #[inline(always)]
    pub const fn set_spr_overflow(&mut self, val: bool) {
        self.status_spr_overflow = val;
    }

    #[inline(always)]
    pub const fn set_spr_zero_hit(&mut self, val: bool) {
        self.status_spr_zero_hit = val;
    }

    #[inline(always)]
    pub const fn set_in_vblank(&mut self, val: bool) {
        self.status_in_vblank = val;
    }

    #[inline(always)]
    pub const fn reset_in_vblank(&mut self) {
        self.status_in_vblank = false;
    }

    // https://www.nesdev.org/wiki/PPU_power_up_state
    pub const fn reset_status(&mut self, kind: ResetKind) {
        if matches!(kind, ResetKind::Hard) {
            self.set_in_vblank(false); // Technically random
            self.set_spr_zero_hit(false);
            self.set_spr_overflow(false); // Technically random
        }
    }
}
