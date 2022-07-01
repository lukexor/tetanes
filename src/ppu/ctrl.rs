use crate::common::{Kind, Reset};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

const NAMETABLE1: u16 = 0x2000;
const NAMETABLE2: u16 = 0x2400;
const NAMETABLE3: u16 = 0x2800;
const NAMETABLE4: u16 = 0x2C00;

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
    #[derive(Default, Serialize, Deserialize)]
    #[must_use]
    pub struct PpuCtrl: u8 {
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

impl PpuCtrl {
    pub const fn new() -> Self {
        Self::from_bits_truncate(0x00)
    }

    #[inline]
    pub fn write(&mut self, val: u8) {
        self.bits = val;
    }

    #[inline]
    #[must_use]
    pub fn nametable_addr(&self) -> u16 {
        match self.bits & 0b11 {
            0b00 => NAMETABLE1,
            0b01 => NAMETABLE2,
            0b10 => NAMETABLE3,
            0b11 => NAMETABLE4,
            _ => unreachable!("impossible nametable_addr"),
        }
    }

    #[inline]
    #[must_use]
    pub const fn vram_increment(&self) -> u16 {
        if self.contains(Self::VRAM_INCREMENT) {
            32
        } else {
            1
        }
    }

    #[inline]
    #[must_use]
    pub const fn spr_select(&self) -> u16 {
        if self.contains(Self::SPR_SELECT) {
            0x1000
        } else {
            0x0000
        }
    }

    #[inline]
    #[must_use]
    pub const fn bg_select(&self) -> u16 {
        if self.contains(Self::BG_SELECT) {
            0x1000
        } else {
            0x0000
        }
    }

    #[inline]
    #[must_use]
    pub const fn spr_height(&self) -> u32 {
        if self.contains(Self::SPR_HEIGHT) {
            16
        } else {
            8
        }
    }

    #[inline]
    #[must_use]
    pub const fn master_slave(&self) -> u8 {
        if self.contains(Self::MASTER_SLAVE) {
            1
        } else {
            0
        }
    }

    #[inline]
    #[must_use]
    pub const fn nmi_enabled(&self) -> bool {
        self.contains(Self::NMI_ENABLE)
    }
}

impl Reset for PpuCtrl {
    // https://www.nesdev.org/wiki/PPU_power_up_state
    fn reset(&mut self, _kind: Kind) {
        self.bits = 0x00;
    }
}
