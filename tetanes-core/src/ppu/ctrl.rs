//! PPUCTRL register implementation.
//!
//! See: <https://wiki.nesdev.com/w/index.php/PPU_registers#PPUCTRL>

use crate::common::{Reset, ResetKind};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

const NAMETABLE1: u16 = 0x2000;
const NAMETABLE2: u16 = 0x2400;
const NAMETABLE3: u16 = 0x2800;
const NAMETABLE4: u16 = 0x2C00;

/// PPUCTRL register.
///
/// See: <https://wiki.nesdev.com/w/index.php/PPU_registers#PPUCTRL>
#[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
#[must_use]
pub struct Ctrl {
    pub spr_select: u16,
    pub bg_select: u16,
    pub spr_height: u32,
    pub master_slave: u8,
    pub nmi_enabled: bool,
    pub nametable_addr: u16,
    pub vram_increment: u16,
    bits: Bits,
}

bitflags! {
    // $2000 PPUCTRL
    //
    // http://wiki.nesdev.com/w/index.php/PPU_registers#PPUCTRL
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

impl Ctrl {
    pub fn new() -> Self {
        let mut ctrl = Self::default();
        ctrl.write(0);
        ctrl
    }

    pub fn write(&mut self, val: u8) {
        self.bits = Bits::from_bits_truncate(val);
        // 0x1000 or 0x0000
        self.spr_select = self.bits.contains(Bits::SPR_SELECT) as u16 * 0x1000;
        // 0x1000 or 0x0000
        self.bg_select = self.bits.contains(Bits::BG_SELECT) as u16 * 0x1000;
        // 16 or 8
        self.spr_height = self.bits.contains(Bits::SPR_HEIGHT) as u32 * 8 + 8;
        // 1 or 0
        self.master_slave = self.bits.contains(Bits::MASTER_SLAVE) as u8;
        self.nmi_enabled = self.bits.contains(Bits::NMI_ENABLE);
        self.nametable_addr = match self.bits.bits() & 0b11 {
            0b00 => NAMETABLE1,
            0b01 => NAMETABLE2,
            0b10 => NAMETABLE3,
            0b11 => NAMETABLE4,
            _ => unreachable!("impossible nametable_addr"),
        };
        // 32 or 1
        self.vram_increment = self.bits.contains(Bits::VRAM_INCREMENT) as u16 * 31 + 1
    }
}

impl Reset for Ctrl {
    // https://www.nesdev.org/wiki/PPU_power_up_state
    fn reset(&mut self, _kind: ResetKind) {
        self.write(0);
    }
}
