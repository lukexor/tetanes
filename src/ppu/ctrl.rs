use crate::common::{Reset, ResetKind};
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
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
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

    pub fn write(&mut self, val: u8) {
        *self = Self::from_bits_truncate(val);
    }

    #[must_use]
    pub fn nametable_addr(&self) -> u16 {
        match self.bits() & 0b11 {
            0b00 => NAMETABLE1,
            0b01 => NAMETABLE2,
            0b10 => NAMETABLE3,
            0b11 => NAMETABLE4,
            _ => unreachable!("impossible nametable_addr"),
        }
    }

    #[must_use]
    pub const fn vram_increment(&self) -> u16 {
        // 32 or 1
        self.contains(Self::VRAM_INCREMENT) as u16 * 31 + 1
    }

    #[must_use]
    pub const fn spr_select(&self) -> u16 {
        // 0x1000 or 0x0000
        self.contains(Self::SPR_SELECT) as u16 * 0x1000
    }

    #[must_use]
    pub const fn bg_select(&self) -> u16 {
        // 0x1000 or 0x0000
        self.contains(Self::BG_SELECT) as u16 * 0x1000
    }

    #[must_use]
    pub const fn spr_height(&self) -> u32 {
        // 16 or 8
        self.contains(Self::SPR_HEIGHT) as u32 * 8 + 8
    }

    #[must_use]
    pub const fn master_slave(&self) -> u8 {
        // 1 or 0
        self.contains(Self::MASTER_SLAVE) as u8
    }

    #[must_use]
    pub const fn nmi_enabled(&self) -> bool {
        self.contains(Self::NMI_ENABLE)
    }
}

impl Reset for PpuCtrl {
    // https://www.nesdev.org/wiki/PPU_power_up_state
    fn reset(&mut self, _kind: ResetKind) {
        *self = Self::empty();
    }
}
