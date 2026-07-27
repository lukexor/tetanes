//! `SxROM`/`MMC1` (Mapper 001).
//!
//! <https://wiki.nesdev.org/w/index.php/MMC1>

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps, Mmc1, Mmc1Revision},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `SxROM`/`MMC1` (Mapper 001).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sxrom {
    pub mmc1: Mmc1,
    pub submapper_num: u8,
    /// 512K carts use a CHR register bit as the high PRG bank bit.
    pub prg_select: bool,
}

impl Sxrom {
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;
    const PRG_BANK_MASK: u8 = 0x0F;
    const PRG_BANK_SELECT_MASK: u8 = 0x10;

    // PPU $0000..=$0FFF 4K CHR Bank Switchable, or 8K across both in 8K mode
    // PPU $1000..=$1FFF 4K CHR Bank Switchable
    // CPU $6000..=$7FFF 8K PRG-RAM, write-protectable
    // CPU $8000..=$BFFF 16K PRG-ROM, switchable or fixed to first depending on mode
    // CPU $C000..=$FFFF 16K PRG-ROM, fixed to last or switchable depending on mode
    pub fn load(cart: &mut Cart, revision: Mmc1Revision) -> Result<Mapper, mapper::Error> {
        let mut sxrom = Self {
            mmc1: Mmc1::new(revision),
            submapper_num: cart.submapper_num(),
            prg_select: cart.prg_rom_size == 0x80000,
        };
        sxrom.sync(&mut cart.memory);
        Ok(sxrom.into())
    }
}

impl Map for Sxrom {
    fn mapper_ops(&self) -> MapperOps {
        // The busy-cycle counter that ignores rapid consecutive writes needs a per-cycle clock;
        // MMC1 has no IRQ.
        MapperOps::CLOCKED
    }

    fn mirroring(&self) -> Mirroring {
        self.mmc1.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        // $6000..=$7FFF is PRG-RAM, already stored by the caller when the window is writable.
        if addr >= 0x8000 && self.mmc1.process_shift_register_write(addr, val) {
            self.sync(memory);
        }
    }

    fn sync(&mut self, memory: &mut Memory) {
        let mmc1 = &self.mmc1;
        // In 4K CHR mode the second CHR register supplies the extra PRG bank bit once it has been
        // the most recently written one.
        let extra_reg = if mmc1.last_chr_reg == 0xC000 && mmc1.chr_mode {
            mmc1.chr1
        } else {
            mmc1.chr0
        };
        let prg_high = if self.prg_select {
            extra_reg & Self::PRG_BANK_SELECT_MASK
        } else {
            0x00
        };

        if self.submapper_num == 5 {
            // SUROM variants with fixed 32K PRG.
            memory.map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
            memory.map_prg(0xC000, Self::PRG_WINDOW, 1, Src::PrgRom);
        } else if mmc1.prg_mode {
            if mmc1.prg_bank_select {
                // $8000 switchable, $C000 fixed to the last bank of the 256K half.
                memory.map_prg(
                    0x8000,
                    Self::PRG_WINDOW,
                    i32::from(mmc1.prg | prg_high),
                    Src::PrgRom,
                );
                memory.map_prg(
                    0xC000,
                    Self::PRG_WINDOW,
                    i32::from(Self::PRG_BANK_MASK | prg_high),
                    Src::PrgRom,
                );
            } else {
                // $8000 fixed to the first bank of the half, $C000 switchable.
                memory.map_prg(0x8000, Self::PRG_WINDOW, i32::from(prg_high), Src::PrgRom);
                memory.map_prg(
                    0xC000,
                    Self::PRG_WINDOW,
                    i32::from(mmc1.prg | prg_high),
                    Src::PrgRom,
                );
            }
        } else {
            // 32K mode ignores the low bank bit.
            let bank = i32::from((mmc1.prg & 0xFE) | prg_high);
            memory.map_prg(0x8000, Self::PRG_WINDOW, bank, Src::PrgRom);
            memory.map_prg(0xC000, Self::PRG_WINDOW, bank + 1, Src::PrgRom);
        }

        if mmc1.chr_mode {
            memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(mmc1.chr0), Src::Chr);
            memory.map_chr(0x1000, Self::CHR_WINDOW, i32::from(mmc1.chr1), Src::Chr);
        } else {
            // 8K mode ignores the low bank bit.
            let bank = i32::from(mmc1.chr0 & 0x1E);
            memory.map_chr(0x0000, Self::CHR_WINDOW, bank, Src::Chr);
            memory.map_chr(0x1000, Self::CHR_WINDOW, bank + 1, Src::Chr);
        }

        // Disabled PRG-RAM reads as open bus and ignores writes, which unmapping gives for free.
        if mmc1.prg_ram_enabled() {
            memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        } else {
            memory.unmap_prg(0x6000, Self::PRG_RAM_WINDOW);
        }

        memory.set_mirroring(mmc1.mirroring);
    }

    fn clock(&mut self) {
        self.mmc1.clock();
    }

    fn reset(&mut self, kind: ResetKind) {
        self.mmc1.reset(kind);
    }
}
