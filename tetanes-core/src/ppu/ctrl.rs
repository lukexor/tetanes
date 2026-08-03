//! PPUCTRL register implementation.
//!
//! See: <https://wiki.nesdev.org/w/index.php/PPU_registers#PPUCTRL>
//!
//! The register's fields live on [`Ppu`] as `ctrl_*` rather than in a struct of their own, so that
//! field order alone decides which of them share the dot loop's cache line - `ctrl_bg_select` is
//! wanted every few dots while fetching, `ctrl_nmi_enabled` once a frame. What the register *does*
//! stays here, in an `impl Ppu` block, the same way a CPU-bus access lives with the state it reads.

// The PPU's internal register and fetch state, whose meaning is the hardware's rather than this
// crate's. Public for embedders and debuggers, not a stable surface - see the module docs on `ppu`.
#![allow(missing_docs)]

use crate::{common::ResetKind, ppu::Ppu};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    // $2000 PPUCTRL
    //
    // https://wiki.nesdev.org/w/index.php/PPU_registers#PPUCTRL
    // VPHB SINN
    // |||| ||++- Nametable Select: 0b00 = $2000 (upper-left); 0b01 = $2400 (upper-right);
    // |||| ||                      0b10 = $2800 (lower-left); 0b11 = $2C00 (lower-right)
    // |||| |||+-   Also For PPUSCROLL: 1 = Add 256 to X scroll
    // |||| ||+--   Also For PPUSCROLL: 1 = Add 240 to Y scroll
    // |||| |+--- VRAM Increment Mode: 0 = add 1, going across; 1 = add 32, going down
    // |||| +---- Sprite Pattern Select for 8x8: 0 = $0000, 1 = $1000, ignored in 8x16 mode
    // |||+------ Background Pattern Select: 0 = $0000, 1 = $1000
    // ||+------- Sprite Height: 0 = 8x8, 1 = 8x16
    // |+-------- PPU Master/Slave: 0 = read from EXT, 1 = write to EXT
    // +--------- NMI Enable: NMI at next vblank: 0 = off, 1: on
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
    #[must_use]
    pub struct Bits: u8 {
        const NAMETABLE1 = 0x01;
        const NAMETABLE2 = 0x02;
        const VRAM_INCREMENT = 0x04;
        const SPR_SELECT = 0x08;
        const BG_SELECT = 0x10;
        const SPR_HEIGHT = 0x20;
        const MASTER_SLAVE = 0x40;
        const NMI_ENABLE = 0x80;
    }
}

impl Ppu {
    /// Decode a $2000 PPUCTRL write into the `ctrl_*` fields.
    pub const fn write_ctrl(&mut self, val: u8) {
        let bits = Bits::from_bits_truncate(val);
        // 0x1000 or 0x0000
        self.ctrl_spr_select = bits.contains(Bits::SPR_SELECT) as u16 * 0x1000;
        // 0x1000 or 0x0000
        self.ctrl_bg_select = bits.contains(Bits::BG_SELECT) as u16 * 0x1000;
        // 16 or 8
        self.ctrl_spr_height = bits.contains(Bits::SPR_HEIGHT) as u16 * 8 + 8;
        // 1 or 0
        self.ctrl_master_slave = bits.contains(Bits::MASTER_SLAVE) as u8;
        self.ctrl_nmi_enabled = bits.contains(Bits::NMI_ENABLE);
        // 32 or 1
        self.ctrl_vram_increment = bits.contains(Bits::VRAM_INCREMENT);
    }

    // https://www.nesdev.org/wiki/PPU_power_up_state
    pub const fn reset_ctrl(&mut self, _kind: ResetKind) {
        self.write_ctrl(0);
    }
}
