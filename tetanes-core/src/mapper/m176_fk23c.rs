//! `Waixing FK23C` / `FS303` (Mapper 176).
//!
//! An MMC3-derived Waixing mapper family with outer PRG/CHR bank registers at
//! `$5xxx`, an extended MMC3 mode that exposes four extra 1KB-CHR/PRG bank
//! registers, four-way mirroring, and a RAM-configuration register at `$A001`
//! (submapper 2). Ported from the nesdev spec and Mesen2's `Fk23C`.
//!
//! <https://www.nesdev.org/wiki/NES_2.0_Mapper_176>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps, mmc3::Mmc3},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `Waixing FK23C`/`FS303` (Mapper 176).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Fk23C {
    pub mmc3: Mmc3,
    /// Extended MMC3 registers `$8`-`$B` (indices 8-11 of the 12-register file).
    pub bank_values_ext: [u8; 4],

    // $5xx0 Mode register
    pub prg_banking_mode: u8,
    pub outer_chr_bank_size: bool,
    pub select_chr_ram: bool,
    pub mmc3_chr_mode: bool,
    pub cnrom_chr_mode: bool,
    // $5xx1 / $5xx2 outer bank base
    pub prg_base_bits: u16,
    pub chr_base_bits: u8,
    // $5xx3 extended-mode enable
    pub extended_mmc3_mode: bool,
    // $A001 RAM-configuration register
    pub wram_bank_select: u8,
    pub ram_in_first_chr_bank: bool,
    pub allow_single_screen_mirroring: bool,
    pub fk23_registers_enabled: bool,
    pub wram_config_enabled: bool,
    pub wram_enabled: bool,
    pub wram_write_protected: bool,
    // $8000 invert bits
    pub invert_prg_a14: bool,
    pub invert_chr_a12: bool,
    pub cnrom_chr_reg: u8,
    pub mirroring_reg: u8,

    pub has_chr_ram: bool,
    /// Whether the cart carries CHR-RAM *alongside* CHR-ROM, which only an NES 2.0 header can
    /// declare. Without it the RAM-select and first-bank-RAM bits have nothing to select.
    pub has_chr_ram_overlay: bool,
    /// Power-on `prg_base_bits`: the 1MB-PRG subtype-1 boot quirk selects the
    /// upper 512KB; everything else boots at 0.
    pub init_prg_base: u16,
    pub mirroring: Mirroring,
}

impl Fk23C {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    const WRAM_BANK: usize = 8 * 1024;

    /// Standard MMC3 registers `$0`-`$7` plus extended `$8`-`$B` power-on values.
    const INIT_REGS: [u8; 12] = [0, 2, 4, 5, 6, 7, 0, 1, 0xFE, 0xFF, 0xFF, 0xFF];

    /// Load `Fk23C` from `Cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let has_chr_ram = cart.chr_rom_size == 0;
        let has_chr_ram_overlay = !has_chr_ram && cart.header.chr_ram_shift > 0;
        let init_prg_base =
            if cart.prg_rom_size == 1024 * 1024 && cart.prg_rom_size == cart.chr_rom_size {
                0x20
            } else {
                0
            };
        let mut fk23c = Self {
            mmc3: Mmc3::default(),
            bank_values_ext: [0; 4],
            prg_banking_mode: 0,
            outer_chr_bank_size: false,
            select_chr_ram: false,
            mmc3_chr_mode: false,
            cnrom_chr_mode: false,
            prg_base_bits: 0,
            chr_base_bits: 0,
            extended_mmc3_mode: false,
            wram_bank_select: 0,
            ram_in_first_chr_bank: false,
            allow_single_screen_mirroring: false,
            fk23_registers_enabled: false,
            wram_config_enabled: false,
            wram_enabled: false,
            wram_write_protected: false,
            invert_prg_a14: false,
            invert_chr_a12: false,
            cnrom_chr_reg: 0,
            mirroring_reg: 0,
            has_chr_ram,
            has_chr_ram_overlay,
            init_prg_base,
            mirroring: cart.mirroring(),
        };
        fk23c.reset(ResetKind::Hard);
        fk23c.update_banks(&mut cart.memory);
        Ok(fk23c.into())
    }

    /// Whether a CHR slot holding `page` reads the 8KB CHR-RAM overlay rather than CHR-ROM.
    ///
    /// The RAM-config register routes bank values 0-7 to RAM (custom fonts); `$5xx0.5` forces all
    /// CHR to RAM. Always false for CHR-RAM-only carts, whose CHR region is already RAM.
    const fn chr_page_uses_overlay(&self, page: usize) -> bool {
        // A CHR-RAM-only cart has no separate overlay - its CHR region is already RAM - and a cart
        // that declares no CHR-RAM has nothing for the RAM-select bit to select. Honouring the bit
        // anyway pointed every CHR fetch at an empty buffer and rendered a black screen.
        if self.has_chr_ram || !self.has_chr_ram_overlay {
            return false;
        }
        if self.select_chr_ram {
            return true;
        }
        // Only the first 8KB (bank values 0-7) route to RAM.
        self.wram_config_enabled && self.ram_in_first_chr_bank && page <= 7
    }

    /// The four 8K PRG bank selections for the current mode.
    const fn prg_pages(&self) -> [usize; 4] {
        let mut pages = [0usize; 4];
        match self.prg_banking_mode {
            0..=2 => {
                // invert_prg_a14 swaps the $8000 and $C000 banks (slots 0 and 2).
                let swap = if self.invert_prg_a14 { 2 } else { 0 };
                if self.extended_mmc3_mode {
                    let outer = (self.prg_base_bits as usize) << 1;
                    pages[swap] = self.mmc3.bank_values[6] as usize | outer;
                    pages[1] = self.mmc3.bank_values[7] as usize | outer;
                    pages[2 ^ swap] = self.bank_values_ext[0] as usize | outer;
                    pages[3] = self.bank_values_ext[1] as usize | outer;
                } else {
                    let inner_mask = 0x3Fusize >> self.prg_banking_mode;
                    let outer = ((self.prg_base_bits as usize) << 1) & !inner_mask;
                    let r6 = self.mmc3.bank_values[6] as usize;
                    let r7 = self.mmc3.bank_values[7] as usize;
                    pages[swap] = (r6 & inner_mask) | outer;
                    pages[1] = (r7 & inner_mask) | outer;
                    pages[2 ^ swap] = (0xFE & inner_mask) | outer;
                    pages[3] = (0xFF & inner_mask) | outer;
                }
            }
            3 => {
                // NROM-128: 16KB mirrored.
                let bank = (self.prg_base_bits as usize) << 1;
                pages = [bank, bank | 1, bank, bank | 1];
            }
            4 => {
                // NROM-256: 32KB.
                let bank = ((self.prg_base_bits as usize) & 0xFFE) << 1;
                pages = [bank, bank | 1, bank | 2, bank | 3];
            }
            _ => (),
        }
        pages
    }

    /// The eight 1K CHR bank selections for the current mode.
    fn chr_pages(&self) -> [usize; 8] {
        let swap = if self.invert_chr_a12 { 0x04 } else { 0 };
        let mut pages = [0usize; 8];
        if !self.mmc3_chr_mode {
            let inner_mask = if self.cnrom_chr_mode {
                if self.outer_chr_bank_size { 1 } else { 3 }
            } else {
                0
            };
            for (i, page) in pages.iter_mut().enumerate() {
                *page = (((self.cnrom_chr_reg & inner_mask) as usize
                    | self.chr_base_bits as usize)
                    << 3)
                    + i;
            }
        } else if self.extended_mmc3_mode {
            let outer = (self.chr_base_bits as usize) << 3;
            let bv = self.mmc3.bank_values;
            let bvx = self.bank_values_ext;
            let regs = [
                bv[0] as usize,  // $0 -> slot 0
                bvx[2] as usize, // $A -> slot 1
                bv[1] as usize,  // $1 -> slot 2
                bvx[3] as usize, // $B -> slot 3
                bv[2] as usize,  // $2 -> slot 4
                bv[3] as usize,  // $3 -> slot 5
                bv[4] as usize,  // $4 -> slot 6
                bv[5] as usize,  // $5 -> slot 7
            ];
            for (slot, page) in regs.into_iter().enumerate() {
                pages[slot ^ swap] = page | outer;
            }
        } else {
            let inner_mask = if self.outer_chr_bank_size { 0x7F } else { 0xFF };
            let outer = ((self.chr_base_bits as usize) << 3) & !inner_mask;
            let bv = self.mmc3.bank_values;
            let regs = [
                ((bv[0] & 0xFE) as usize & inner_mask) | outer,
                ((bv[0] | 0x01) as usize & inner_mask) | outer,
                ((bv[1] & 0xFE) as usize & inner_mask) | outer,
                ((bv[1] | 0x01) as usize & inner_mask) | outer,
                (bv[2] as usize & inner_mask) | outer,
                (bv[3] as usize & inner_mask) | outer,
                (bv[4] as usize & inner_mask) | outer,
                (bv[5] as usize & inner_mask) | outer,
            ];
            for (slot, page) in regs.into_iter().enumerate() {
                pages[slot ^ swap] = page;
            }
        }
        pages
    }

    const fn update_mirroring(&mut self) {
        let mask = if self.allow_single_screen_mirroring {
            0x03
        } else {
            0x01
        };
        self.mirroring = match self.mirroring_reg & mask {
            0 => Mirroring::Vertical,
            1 => Mirroring::Horizontal,
            2 => Mirroring::SingleScreenA,
            _ => Mirroring::SingleScreenB,
        };
    }

    const fn update_state(&mut self) {
        self.update_mirroring();
    }

    /// Whether WRAM writes are allowed. The `$A001` write-protect bit only
    /// applies in the non-RAM-config mode; RAM-config banking is always R/W.
    const fn wram_writable(&self) -> bool {
        self.wram_config_enabled || !self.wram_write_protected
    }
}

impl Map for Fk23C {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ | MapperOps::WATCHES_PPU_BUS
    }

    /// MMC3-derived, so it counts scanlines from A12 rising edges.
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
        match addr {
            // $5xxx is the register window when FK23C registers are enabled (or RAM config is
            // off); otherwise it is banked WRAM, and the guard falls through to `_` - which still
            // reaches the `self.update_banks(memory)` below, exactly as an empty arm body would.
            0x4000..=0x5FFF if self.fk23_registers_enabled || !self.wram_config_enabled => {
                // Solder-pad address mask: a register write must have the
                // $5010 bits set (mask $Fxx3 selects $5xx0-$5xx3).
                if addr & 0x5010 != 0x5010 {
                    return;
                }
                match addr & 0x03 {
                    0 => {
                        self.prg_banking_mode = val & 0x07;
                        self.outer_chr_bank_size = val & 0x10 != 0;
                        self.select_chr_ram = val & 0x20 != 0;
                        self.mmc3_chr_mode = val & 0x40 == 0;
                        self.prg_base_bits = (self.prg_base_bits & !0x180)
                            | (u16::from(val & 0x80) << 1)
                            | (u16::from(val & 0x08) << 4);
                    }
                    1 => {
                        self.prg_base_bits = (self.prg_base_bits & !0x7F) | u16::from(val & 0x7F);
                    }
                    2 => {
                        self.prg_base_bits =
                            (self.prg_base_bits & !0x200) | (u16::from(val & 0x40) << 3);
                        self.chr_base_bits = val;
                        self.cnrom_chr_reg = 0;
                    }
                    _ => {
                        self.extended_mmc3_mode = val & 0x02 != 0;
                        self.cnrom_chr_mode = val & 0x44 != 0;
                    }
                }
                self.update_state();
            }
            // WRAM stores already happened in `Bus`.
            0x6000..=0x7FFF => (),
            0x8000..=0xFFFF => {
                // CNROM latch: any $8000-$9FFF or $C000-$FFFF write sets the CHR
                // register. Tracked with a single rebank at the end to avoid
                // rebanking twice when it coincides with an MMC3 register write.
                let mut rebank = false;
                if self.cnrom_chr_mode && (addr <= 0x9FFF || addr >= 0xC000) {
                    self.cnrom_chr_reg = val & 0x03;
                    rebank = true;
                }
                match addr & 0xE001 {
                    0x8000 => {
                        self.invert_prg_a14 = val & 0x40 != 0;
                        self.invert_chr_a12 = val & 0x80 != 0;
                        self.mmc3.write_bank_select(val);
                        rebank = true;
                    }
                    0x8001 => {
                        let reg = self.mmc3.bank_select
                            & if self.extended_mmc3_mode { 0x0F } else { 0x07 };
                        if reg < 8 {
                            self.mmc3.bank_values[reg as usize] = val;
                        } else if reg < 12 {
                            self.bank_values_ext[(reg - 8) as usize] = val;
                        }
                        rebank = true;
                    }
                    0xA000 => {
                        // Mirroring only; banking is unaffected.
                        self.mirroring_reg = val & 0x03;
                        self.update_mirroring();
                    }
                    0xA001 => {
                        // Bits other than 6-7 are ignored unless bit 5 is set.
                        let val = if val & 0x20 == 0 { val & 0xC0 } else { val };
                        self.wram_bank_select = val & 0x03;
                        self.ram_in_first_chr_bank = val & 0x04 != 0;
                        self.allow_single_screen_mirroring = val & 0x08 != 0;
                        self.wram_config_enabled = val & 0x20 != 0;
                        self.fk23_registers_enabled = val & 0x40 != 0;
                        self.wram_write_protected = val & 0x40 != 0;
                        self.wram_enabled = val & 0x80 != 0;
                        // Mirroring is derived state and has to be recomputed here, because the
                        // single-screen unlock bit changes the mask. The CHR-RAM overlay and the
                        // WRAM windows are chosen at map time rather than per access, so they
                        // depend on the unconditional `update_banks` at the end of this function.
                        self.update_mirroring();
                    }
                    0xC000 => self.mmc3.write_irq_latch(val),
                    0xC001 => self.mmc3.write_irq_reload(),
                    0xE000 => self.mmc3.write_irq_disable(),
                    0xE001 => self.mmc3.write_irq_enable(),
                    _ => unreachable!("impossible address"),
                }
                if rebank {
                    self.update_state();
                }
            }
            _ => (),
        }
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        // Each CHR slot independently reads CHR-ROM or the 8K CHR-RAM overlay, depending on the
        // RAM-config register and the slot's own bank value.
        for (slot, page) in self.chr_pages().into_iter().enumerate() {
            let src = if self.chr_page_uses_overlay(page) {
                Src::ExRam
            } else {
                Src::Chr
            };
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, page as i32, src);
        }

        for (slot, page) in self.prg_pages().into_iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, page as i32, Src::PrgRom);
        }

        // WRAM is up to 32K in 8K pages, optionally also visible at $4000. Only a window that is
        // actually mapped gets its writable flag set: an unmapped page carries offset 0, the
        // reserved zero-filled page, so write-enabling one would route $4100-$7FFF stores - the
        // board's own $5xx0-$5xx3 register writes among them - into the block every unmapped read
        // in both address spaces returns.
        let writable = self.wram_writable();
        if self.wram_config_enabled {
            let bank = i32::from(self.wram_bank_select);
            memory.map_prg(0x4000, Self::WRAM_BANK, (bank + 1) & 0x03, Src::PrgRam);
            memory.map_prg(0x6000, Self::WRAM_BANK, bank, Src::PrgRam);
            memory.set_prg_writable(0x4000, Self::WRAM_BANK, writable);
            memory.set_prg_writable(0x6000, Self::WRAM_BANK, writable);
        } else {
            memory.unmap_prg(0x4000, Self::WRAM_BANK);
            if self.wram_enabled {
                memory.map_prg(0x6000, Self::WRAM_BANK, 0, Src::PrgRam);
                memory.set_prg_writable(0x6000, Self::WRAM_BANK, writable);
            } else {
                memory.unmap_prg(0x6000, Self::WRAM_BANK);
            }
        }

        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        self.mmc3.clock();
    }

    fn reset(&mut self, kind: ResetKind) {
        self.mmc3.reset(kind);
        self.mmc3.bank_values.copy_from_slice(&Self::INIT_REGS[..8]);
        self.bank_values_ext.copy_from_slice(&Self::INIT_REGS[8..]);
        self.prg_banking_mode = 0;
        self.outer_chr_bank_size = false;
        self.select_chr_ram = false;
        self.mmc3_chr_mode = true;
        self.cnrom_chr_mode = false;
        self.prg_base_bits = self.init_prg_base;
        self.chr_base_bits = 0;
        self.extended_mmc3_mode = false;
        self.wram_bank_select = 0;
        self.ram_in_first_chr_bank = false;
        self.allow_single_screen_mirroring = false;
        self.fk23_registers_enabled = false;
        self.wram_config_enabled = false;
        self.wram_enabled = false;
        self.wram_write_protected = false;
        self.invert_prg_a14 = false;
        self.invert_chr_a12 = false;
        self.cnrom_chr_reg = 0;
        self.mirroring_reg = 0;
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 256K PRG-ROM (32 8K banks), 32K PRG-RAM (4 WRAM banks), 64K CHR-ROM (64 1K banks).
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(256 * 1024, 32 * 1024, 64 * 1024);
        cart.header.mapper_num = 176;
        let mapper = Fk23C::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Selects an MMC3 register and writes its value.
    fn bank(mapper: &mut Mapper, cart: &mut Cart, reg: u8, val: u8) {
        write(mapper, cart, 0x8000, reg);
        write(mapper, cart, 0x8001, val);
    }

    /// Writes one of the four `$5xx0-$5xx3` mode registers. The solder-pad mask means only
    /// addresses with the `$5010` bits set decode as registers at all.
    fn mode(mapper: &mut Mapper, cart: &mut Cart, reg: u16, val: u8) {
        write(mapper, cart, 0x5010 | reg, val);
    }

    /// The board boots as a plain MMC3: $C000/$E000 fixed to the last two banks by the $FE/$FF
    /// power-on register values.
    #[test]
    fn powers_on_as_an_mmc3_with_the_last_banks_fixed() {
        let (mapper, cart) = load();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "R6 = bank 0");
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 8, "R7 = bank 1");
        // $FE and $FF masked to 6 bits are banks 62 and 63, which wrap onto this cart's 32.
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), (62 % 32) * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), (63 % 32) * 8, "last bank");
    }

    /// The MMC3 registers still bank PRG and CHR the ordinary way.
    #[test]
    fn mmc3_registers_bank_prg_and_chr() {
        let (mut mapper, mut cart) = load();

        bank(&mut mapper, &mut cart, 6, 3);
        bank(&mut mapper, &mut cart, 7, 5);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 5 * 8);

        // R0 and R1 are 2K windows: the low bit of the register is forced.
        bank(&mut mapper, &mut cart, 0, 10);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 10);
        assert_eq!(chr_peek(&mapper, &cart, 0x0400), 0x80 | 11, "2K window");

        // R2-R5 are 1K windows.
        bank(&mut mapper, &mut cart, 2, 20);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | 20);
    }

    /// Bits 6 and 7 of $8000 are this board's own inversion bits, swapping the PRG and CHR halves.
    #[test]
    fn bits_6_and_7_of_8000_invert_the_prg_and_chr_halves() {
        let (mut mapper, mut cart) = load();
        bank(&mut mapper, &mut cart, 6, 3);

        // Bit 6 swaps the $8000 and $C000 slots.
        write(&mut mapper, &mut cart, 0x8000, 0x40);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 3 * 8, "R6 moved to $C000");
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), (62 % 32) * 8);

        // Bit 7 swaps the two 4K CHR halves.
        let (mut mapper, mut cart) = load();
        let before: Vec<u8> = (0..8).map(|s| chr_peek(&mapper, &cart, s * 1024)).collect();
        write(&mut mapper, &mut cart, 0x8000, 0x80);
        let after: Vec<u8> = (0..8).map(|s| chr_peek(&mapper, &cart, s * 1024)).collect();
        assert_eq!(after[..4], before[4..], "the halves swapped");
        assert_eq!(after[4..], before[..4]);
    }

    /// PRG mode 3 is NROM-128: one 16K bank from the outer base, mirrored into both halves.
    #[test]
    fn prg_mode_3_is_nrom_128_mirrored_from_the_outer_base() {
        let (mut mapper, mut cart) = load();
        mode(&mut mapper, &mut cart, 1, 3); // outer base
        mode(&mut mapper, &mut cart, 0, 3); // banking mode 3

        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 6 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 7 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 6 * 8, "mirrored");
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 7 * 8, "mirrored");
    }

    /// PRG mode 4 is NROM-256: one 32K bank, ignoring the low bit of the outer base.
    #[test]
    fn prg_mode_4_is_nrom_256() {
        let (mut mapper, mut cart) = load();
        mode(&mut mapper, &mut cart, 1, 4);
        mode(&mut mapper, &mut cart, 0, 4);

        for (slot, bank) in [8, 9, 10, 11].into_iter().enumerate() {
            let addr = 0x8000 + (slot * 0x2000) as u16;
            assert_eq!(prg_peek(&mapper, &cart, addr), bank * 8, "slot {slot}");
        }
    }

    /// A register write that misses the solder-pad mask is not a register write at all.
    #[test]
    fn the_5010_solder_pad_mask_gates_the_mode_registers() {
        let (mut mapper, mut cart) = load();

        // $5000 lacks the $10 bit, so this must not select NROM-128 mode.
        write(&mut mapper, &mut cart, 0x5000, 3);
        assert_eq!(
            prg_peek(&mapper, &cart, 0xE000),
            (63 % 32) * 8,
            "still in MMC3 mode"
        );

        mode(&mut mapper, &mut cart, 0, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 8, "now NROM-128");
    }

    /// $A001 controls WRAM. In the banked RAM-config mode it appears at $4000 as well as $6000,
    /// one bank apart - which is the only reason this board maps anything at $4000.
    ///
    /// Probed at $4100 rather than $4000: the window starts at $4000, but the CPU bus decodes
    /// $4000-$40FF to the APU and the controller ports, so those 256 bytes never reach the board.
    #[test]
    fn a001_controls_wram_mapping_and_the_4000_window() {
        let (mut mapper, mut cart) = load();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0, "unmapped at power-on");

        // Bit 7 alone: plain 8K WRAM at $6000, nothing at $4000.
        write(&mut mapper, &mut cart, 0xA001, 0x80);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A);
        assert_eq!(prg_peek(&mapper, &cart, 0x4100), 0, "no $4000 window");

        // Bit 5 turns on RAM-config banking, which also opens the $4000 window.
        write(&mut mapper, &mut cart, 0xA001, 0x20);
        cart.memory.prg_write(0x6000, 0x11);
        cart.memory.prg_write(0x4100, 0x22);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x11);
        assert_eq!(prg_peek(&mapper, &cart, 0x4100), 0x22);
        assert_ne!(
            prg_peek(&mapper, &cart, 0x4100),
            prg_peek(&mapper, &cart, 0x6000),
            "$4000 is the next WRAM bank, not a mirror"
        );
    }

    /// At power-on both WRAM windows are unmapped while WRAM is nominally writable, and the board's
    /// own mode registers live at $5xx0-$5xx3. An unmapped page carries offset 0 - the reserved
    /// zero page every unmapped read in both address spaces returns - so write-enabling one would
    /// turn each register write into a store that dirties it.
    #[test]
    fn register_writes_leave_the_reserved_zero_page_alone() {
        let (mut mapper, mut cart) = load();
        mode(&mut mapper, &mut cart, 1, 0x7F);
        write(&mut mapper, &mut cart, 0x5000, 0xA5);

        for addr in [0x4100, 0x5000, 0x5011, 0x6000, 0x7FFF] {
            assert_eq!(prg_peek(&mapper, &cart, addr), 0, "${addr:04X} reads as 0");
        }

        // Over-protecting would be the other bug: once $A001 maps WRAM there, it still takes writes.
        write(&mut mapper, &mut cart, 0xA001, 0x20);
        write(&mut mapper, &mut cart, 0x6000, 0x3C);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x3C, "mapped WRAM is R/W");
    }

    /// Single-screen mirroring is only reachable once $A001 bit 3 unlocks it; without it the
    /// mirroring register is masked to one bit.
    #[test]
    fn single_screen_mirroring_needs_unlocking_by_a001() {
        let (mut mapper, mut cart) = load();

        // Mirroring 2 would be single-screen A, but the mask keeps it at vertical.
        write(&mut mapper, &mut cart, 0xA000, 0x02);
        cart.memory.chr_write(0x2000, 0x11);
        assert_eq!(chr_peek(&mapper, &cart, 0x2800), 0x11, "still vertical");

        write(&mut mapper, &mut cart, 0xA001, 0x20 | 0x08); // unlock
        write(&mut mapper, &mut cart, 0xA000, 0x02);
        cart.memory.chr_write(0x2000, 0x33);
        for nt in [0x2400, 0x2800, 0x2C00] {
            assert_eq!(chr_peek(&mapper, &cart, nt), 0x33, "single screen A");
        }
    }

    /// The MMC3 scanline counter still drives the IRQ, clocked from A12 rising edges rather than
    /// from the CPU clock.
    #[test]
    fn the_mmc3_scanline_irq_still_works() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xC000, 2); // latch
        write(&mut mapper, &mut cart, 0xC001, 0); // reload
        write(&mut mapper, &mut cart, 0xE001, 0); // enable

        // Each low-to-high A12 transition is one scanline. A12 has to stay low for several CPU
        // clocks first, which is how the real chip rejects the rapid toggling of a normal fetch
        // pattern - so the clocks here are load-bearing.
        // `a12_low_clock == 0` doubles as "A12 is not currently low", so the very first transition
        // out of a zero master clock is swallowed. Advance past it before measuring.
        mapper.clock();

        let scanline = |mapper: &mut Mapper, cart: &mut Cart| {
            mapper.ppu_bus_addr(&mut cart.memory, 0x0000);
            for _ in 0..8 {
                mapper.clock();
            }
            mapper.ppu_bus_addr(&mut cart.memory, 0x1000);
        };

        // The first edge loads the latch, then it counts 2 down to 0.
        for line in 0..2 {
            scanline(&mut mapper, &mut cart);
            assert!(!mapper.irq_pending(), "too early, at line {line}");
        }
        scanline(&mut mapper, &mut cart);
        assert!(mapper.irq_pending(), "fires after the latch counts down");
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// [`Bus::rebuild_mapper_state`](crate::bus::Bus::rebuild_mapper_state) relies on after a
    /// save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xA001, 0x20);
        bank(&mut mapper, &mut cart, 6, 3);
        bank(&mut mapper, &mut cart, 0, 10);
        mode(&mut mapper, &mut cart, 2, 1);

        let sample = |mapper: &Mapper, cart: &Cart| -> Vec<u8> {
            [0x4100, 0x6000, 0x8000, 0xA000, 0xC000, 0xE000]
                .into_iter()
                .map(|addr| prg_peek(mapper, cart, addr))
                .chain((0..8).map(|slot| chr_peek(mapper, cart, slot * 1024)))
                .collect()
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
    }
}
