//! `TxROM`/`MMC3` (Mappers 004, 076, 088, 095, 118, 119, 154, 206).
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
    /// Mapper 119 (TQROM) selects its 8K of CHR-RAM over CHR-ROM with bit 6 of a CHR bank.
    const TQROM_CHR_RAM: u8 = 0x40;
    /// Mapper 119's CHR-RAM is eight 1K banks.
    const TQROM_CHR_RAM_MASK: u8 = 0x07;

    // PPU $0000..=$1FFF 8x 1K CHR Banks (or 4x 2K for mapper 076)
    // PPU $2000..=$2FFF Nametables, picked by CHR bank bit 7 for mapper 118
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
        // TQROM keeps its CHR-RAM in ExRAM, since `Src::Chr` is already its CHR-ROM.
        let tqrom = self.mapper_num == 119;
        let source = |reg: u8| {
            if tqrom && reg & Self::TQROM_CHR_RAM == Self::TQROM_CHR_RAM {
                (Src::ExRam, reg & Self::TQROM_CHR_RAM_MASK)
            } else {
                (Src::Chr, reg)
            }
        };
        // The two 2K windows are expressed as pairs of consecutive 1K pages, matching the
        // hardware's "ignore the low bank bit" behavior.
        let pair = |memory: &mut Memory, addr: u16, reg: u8| {
            let (src, reg) = source(reg);
            let bank = i32::from(reg & 0xFE);
            memory.map_chr(addr, Self::CHR_WINDOW, bank, src);
            memory.map_chr(addr + 0x0400, Self::CHR_WINDOW, bank + 1, src);
        };
        let single = |memory: &mut Memory, addr: u16, reg: u8| {
            let (src, reg) = source(reg);
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(reg), src);
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

    /// Point each nametable at the CIRAM page that bit 7 of its CHR bank register selects.
    //
    // TxSROM wires CIRAM A10 to the MMC3's CHR A17 output. A nametable fetch has A12 clear, so it
    // decodes like a fetch from $0000-$0FFF: R0 and R1 in the normal CHR mode, R2-R5 inverted.
    fn update_txsrom_nametables(&self, memory: &mut Memory) {
        let chr = self.mmc3.bank_values;
        let regs = if self.mmc3.bank_select & Self::CHR_INVERSION_MASK == Self::CHR_INVERSION_MASK {
            [chr[2], chr[3], chr[4], chr[5]]
        } else {
            [chr[0], chr[0], chr[1], chr[1]]
        };
        for (nametable, reg) in regs.into_iter().enumerate() {
            let addr = 0x2000 + (nametable * Self::CHR_WINDOW) as u16;
            let bank = i32::from(reg >> 7);
            memory.map_chr(addr, Self::CHR_WINDOW, bank, Src::CiRam);
            // $3000-$3EFF mirrors $2000-$2EFF.
            memory.map_chr(addr + 0x1000, Self::CHR_WINDOW, bank, Src::CiRam);
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
        self.mmc3.irq_pending
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
                    // Four-screen carts wire their own nametable RAM and ignore this register, and
                    // TxSROM picks nametables from the CHR banks instead.
                    if self.mirroring != Mirroring::FourScreen && self.mapper_num != 118 {
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
        if self.mapper_num == 118 {
            self.update_txsrom_nametables(memory);
        } else {
            // Four-screen carts get 4K of CIRAM from the cart, so the nametables simply map to
            // four distinct pages, with no separate buffer to route them through.
            memory.set_mirroring(self.mirroring);
        }
    }

    fn clock(&mut self) {
        self.mmc3.clock();
    }

    fn reset(&mut self, kind: ResetKind) {
        self.mmc3.reset(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, write};
    use crate::memory::PAGE_SIZE;

    /// CIRAM's first page reads as this.
    const CIRAM_A: u8 = 0x11;
    /// CIRAM's second page reads as this.
    const CIRAM_B: u8 = 0x22;

    /// 64K PRG-ROM, 8K PRG-RAM and 64K page-indexed CHR-ROM, loaded as `mapper_num`. CIRAM pages
    /// read as [`CIRAM_A`] and [`CIRAM_B`], and ExRAM page `n` as `0xE0 | n`.
    fn txrom(mapper_num: u16) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(64 * 1024, 8 * 1024, 64 * 1024);
        cart.header.mapper_num = mapper_num;
        let mapper = Txrom::load(&mut cart).expect("valid mapper");
        let ciram = cart.memory.region_mut(Src::CiRam);
        ciram[..PAGE_SIZE].fill(CIRAM_A);
        ciram[PAGE_SIZE..].fill(CIRAM_B);
        for (page, data) in cart
            .memory
            .region_mut(Src::ExRam)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            data.fill(0xE0 | page as u8);
        }
        (mapper, cart)
    }

    /// Writes `val` to bank register `reg`, with `bank_select`'s mode bits.
    fn bank(mapper: &mut Mapper, cart: &mut Cart, bank_select: u8, reg: u8, val: u8) {
        write(mapper, cart, 0x8000, bank_select | reg);
        write(mapper, cart, 0x8001, val);
    }

    fn nametables(mapper: &Mapper, cart: &Cart) -> [u8; 4] {
        [0x2000, 0x2400, 0x2800, 0x2C00].map(|addr| chr_peek(mapper, cart, addr))
    }

    /// In the normal CHR mode, R0's bit 7 picks the CIRAM page for the first two nametables and
    /// R1's for the last two. The mirroring register does nothing.
    #[test]
    fn txsrom_nametables_follow_r0_and_r1() {
        let (mut mapper, mut cart) = txrom(118);
        bank(&mut mapper, &mut cart, 0x00, 0, 0x80);
        bank(&mut mapper, &mut cart, 0x00, 1, 0x00);
        let expected = [CIRAM_B, CIRAM_B, CIRAM_A, CIRAM_A];
        assert_eq!(nametables(&mapper, &cart), expected);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x3000),
            CIRAM_B,
            "$3000 mirrors $2000"
        );

        write(&mut mapper, &mut cart, 0xA000, 0x01);
        assert_eq!(nametables(&mapper, &cart), expected, "$A000 is ignored");
    }

    /// With CHR inversion, R2-R5 pick one nametable each.
    #[test]
    fn txsrom_nametables_follow_r2_to_r5_when_inverted() {
        let (mut mapper, mut cart) = txrom(118);
        for (reg, val) in [(2, 0x80), (3, 0x00), (4, 0x00), (5, 0x80)] {
            bank(&mut mapper, &mut cart, 0x80, reg, val);
        }
        assert_eq!(
            nametables(&mapper, &cart),
            [CIRAM_B, CIRAM_A, CIRAM_A, CIRAM_B]
        );
    }

    /// Bit 6 of a CHR bank selects TQROM's 8K of CHR-RAM, banked by the low three bits.
    #[test]
    fn tqrom_bit_6_selects_chr_ram() {
        let (mut mapper, mut cart) = txrom(119);
        bank(&mut mapper, &mut cart, 0x00, 0, 0x42);
        bank(&mut mapper, &mut cart, 0x00, 2, 0x4B);
        bank(&mut mapper, &mut cart, 0x00, 3, 0x05);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0000),
            0xE2,
            "2K RAM pair, low half"
        );
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0400),
            0xE3,
            "2K RAM pair, high half"
        );
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0xE3, "$4B is RAM bank 3");
        assert_eq!(chr_peek(&mapper, &cart, 0x1400), 0x85, "ROM page 5");

        cart.memory.chr_write(0x1000, 0x55);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x1000),
            0x55,
            "CHR-RAM is writable"
        );
    }

    /// Plain MMC3 treats bit 6 as part of the CHR-ROM bank number.
    #[test]
    fn mmc3_bit_6_is_a_rom_bank_bit() {
        let (mut mapper, mut cart) = txrom(4);
        bank(&mut mapper, &mut cart, 0x00, 2, 0x43);
        // 64 pages of CHR-ROM, so bank $43 wraps to page 3.
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x83);
    }
}
