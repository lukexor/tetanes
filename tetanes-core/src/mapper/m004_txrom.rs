//! `TxROM`/`MMC3` (Mappers 004, 076, 088, 095, 154, 206).
//!
//! <https://wiki.nesdev.org/w/index.php/MMC3>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps, Mmc3, Mmc3Revision},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `TxROM`/`MMC3` (Mapper 004).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Txrom {
    pub mmc3: Mmc3,
    pub mirroring: Mirroring,
    pub mapper_num: u16,
    pub submapper_num: u8,
}

impl Txrom {
    const PRG_WINDOW: usize = 8 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    /// Mapper 076 banks CHR in 2K rather than 1K windows.
    const CHR_WINDOW_76: usize = 2048;
    const PRG_MODE_MASK: u8 = 0x40;
    const CHR_INVERSION_MASK: u8 = 0x80;

    // PPU $0000..=$1FFF 8x 1K CHR Banks (or 4x 2K for mapper 076)
    // CPU $6000..=$7FFF 8K PRG-RAM
    // CPU $8000..=$FFFF 4x 8K PRG-ROM Banks, two switchable and two fixed depending on mode
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut txrom = Self {
            mmc3: Mmc3::default(),
            mirroring: cart.mirroring(),
            mapper_num: cart.mapper_num(),
            submapper_num: cart.submapper_num(),
        };
        txrom.update_banks(&mut cart.memory);
        Ok(txrom.into())
    }

    pub const fn bank_register(&self, index: usize) -> u8 {
        self.mmc3.bank_values[index]
    }

    pub const fn set_revision(&mut self, rev: Mmc3Revision) {
        self.mmc3.set_revision(rev);
    }

    #[inline]
    const fn apply_prg_write_masks(&self, addr: &mut u16, val: &mut u8) {
        *addr &= 0x8001;
        if *addr == 0x8000 {
            *val &= 0x3F;
        }
    }

    fn update_prg_banks(&self, memory: &mut Memory) {
        let prg_lo = i32::from(self.mmc3.bank_values[6]);
        let prg_hi = i32::from(self.mmc3.bank_values[7]);
        // -1 is the last bank, -2 the one before it.
        if self.mmc3.bank_select & Self::PRG_MODE_MASK == Self::PRG_MODE_MASK {
            memory.map_prg(0x8000, Self::PRG_WINDOW, -2, Src::PrgRom);
            memory.map_prg(0xA000, Self::PRG_WINDOW, prg_hi, Src::PrgRom);
            memory.map_prg(0xC000, Self::PRG_WINDOW, prg_lo, Src::PrgRom);
        } else {
            memory.map_prg(0x8000, Self::PRG_WINDOW, prg_lo, Src::PrgRom);
            memory.map_prg(0xA000, Self::PRG_WINDOW, prg_hi, Src::PrgRom);
            memory.map_prg(0xC000, Self::PRG_WINDOW, -2, Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
    }

    fn update_chr_banks(&mut self, memory: &mut Memory) {
        if self.mapper_num == 76 {
            for (slot, reg) in (2..6).enumerate() {
                let addr = (slot * Self::CHR_WINDOW_76) as u16;
                let bank = i32::from(self.mmc3.bank_values[reg]);
                memory.map_chr(addr, Self::CHR_WINDOW_76, bank, Src::Chr);
            }
            return;
        }
        if matches!(self.mapper_num, 88 | 154) {
            let regs = &mut self.mmc3.bank_values;
            regs[0] &= 0x3F;
            regs[1] &= 0x3F;
            for reg in regs.iter_mut().take(6).skip(2) {
                *reg |= 0x40;
            }
        }

        let chr = self.mmc3.bank_values;
        // The two 2K windows are expressed as pairs of consecutive 1K pages, matching the
        // hardware's "ignore the low bank bit" behaviour.
        let pair = |memory: &mut Memory, addr: u16, reg: u8| {
            let bank = i32::from(reg & 0xFE);
            memory.map_chr(addr, Self::CHR_WINDOW, bank, Src::Chr);
            memory.map_chr(addr + 0x0400, Self::CHR_WINDOW, bank + 1, Src::Chr);
        };
        let single = |memory: &mut Memory, addr: u16, reg: u8| {
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(reg), Src::Chr);
        };

        if self.mmc3.bank_select & Self::CHR_INVERSION_MASK == Self::CHR_INVERSION_MASK {
            single(memory, 0x0000, chr[2]);
            single(memory, 0x0400, chr[3]);
            single(memory, 0x0800, chr[4]);
            single(memory, 0x0C00, chr[5]);
            pair(memory, 0x1000, chr[0]);
            pair(memory, 0x1800, chr[1]);
        } else {
            pair(memory, 0x0000, chr[0]);
            pair(memory, 0x0800, chr[1]);
            single(memory, 0x1000, chr[2]);
            single(memory, 0x1400, chr[3]);
            single(memory, 0x1800, chr[4]);
            single(memory, 0x1C00, chr[5]);
        }
    }
}

impl Map for Txrom {
    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("Bank select", u32::from(self.mmc3.bank_select)));
        for (slot, bank) in self.mmc3.bank_values.iter().enumerate() {
            out.push((
                ["R0", "R1", "R2", "R3", "R4", "R5", "R6", "R7"][slot],
                u32::from(*bank),
            ));
        }
        out.push(("IRQ latch", u32::from(self.mmc3.irq_latch)));
        out.push(("IRQ counter", u32::from(self.mmc3.irq_counter)));
        out.push(("IRQ enabled", u32::from(self.mmc3.irq_enabled)));
        out.push(("IRQ pending", u32::from(self.mmc3.irq_pending)));
    }
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ | MapperOps::WATCHES_PPU_BUS
    }

    /// MMC3 counts scanlines from A12 rising edges on the PPU bus.
    fn ppu_bus_addr(&mut self, _memory: &mut Memory, addr: u16) {
        self.mmc3.clock_irq(addr);
    }

    fn irq_pending(&self) -> bool {
        self.mmc3.irq_pending()
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        let (mut addr, mut val) = (addr, val);
        let mut banking_changed = false;
        let mut mirroring_changed = false;
        match self.mapper_num {
            76 | 88 | 95 | 206 => self.apply_prg_write_masks(&mut addr, &mut val),
            154 => {
                self.mirroring = if val & 0x40 == 0x40 {
                    Mirroring::SingleScreenB
                } else {
                    Mirroring::SingleScreenA
                };
                mirroring_changed = true;
                self.apply_prg_write_masks(&mut addr, &mut val);
            }
            _ => (),
        }

        if addr >= 0x8000 {
            match addr & 0xE001 {
                0x8000 => {
                    self.mmc3.write_bank_select(val);
                    banking_changed = true;
                }
                0x8001 => {
                    self.mmc3.write_bank_data(val);
                    banking_changed = true;
                }
                0xA000 => {
                    // Four-screen carts wire their own nametable RAM and ignore this register.
                    if self.mirroring != Mirroring::FourScreen {
                        self.mirroring = if val & 0x01 == 0x01 {
                            Mirroring::Horizontal
                        } else {
                            Mirroring::Vertical
                        };
                        mirroring_changed = true;
                    }
                }
                // $A001 is PRG-RAM protect, which is not emulated.
                0xA001 => (),
                0xC000 => self.mmc3.write_irq_latch(val),
                0xC001 => self.mmc3.write_irq_reload(),
                0xE000 => self.mmc3.write_irq_disable(),
                0xE001 => self.mmc3.write_irq_enable(),
                _ => unreachable!("impossible address"),
            }
        }

        if self.mapper_num == 95 && addr & 0x01 == 0x01 {
            let nametable1 = (self.bank_register(0) >> 5) & 0x01;
            let nametable2 = (self.bank_register(1) >> 5) & 0x01;
            self.mirroring = match (nametable1, nametable2) {
                (0, 0) => Mirroring::SingleScreenA,
                (1, 1) => Mirroring::SingleScreenB,
                _ => Mirroring::Horizontal,
            };
            mirroring_changed = true;
        }

        // A game arms the IRQ counter once a scanline, and those four registers feed nothing that
        // is banked; rebuilding all 64 page entries for them is pure cost. A mirroring write moves
        // only the eight nametable pages.
        if banking_changed {
            self.update_banks(memory);
        } else if mirroring_changed {
            memory.set_mirroring(self.mirroring);
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        self.update_prg_banks(memory);
        self.update_chr_banks(memory);
        // Four-screen carts get 4K of CIRAM from the cart, so the nametables simply map to four
        // distinct pages, with no separate buffer to route them through.
        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        self.mmc3.clock();
    }

    fn reset(&mut self, kind: ResetKind) {
        self.mmc3.reset(kind);
    }
}
