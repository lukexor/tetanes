//! `Waixing FK23C` / `FS303` (Mapper 176).
//!
//! An MMC3-derived Waixing mapper family with outer PRG/CHR bank registers at
//! `$5xxx`, an extended MMC3 mode that exposes four extra 1KB-CHR/PRG bank
//! registers, four-way mirroring, and a RAM-configuration register at `$A001`
//! (submapper 2). Ported from the nesdev spec and Mesen2's `Fk23C`.
//!
//! <https://www.nesdev.org/wiki/NES_2.0_Mapper_176>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sram},
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
        fk23c.sync(&mut cart.memory);
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
        self.mmc3.irq_pending()
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr {
            0x4000..=0x5FFF => {
                // $5xxx is the register window when FK23C registers are enabled
                // (or RAM config is off); otherwise it is banked WRAM.
                if self.fk23_registers_enabled || !self.wram_config_enabled {
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
                            self.prg_base_bits =
                                (self.prg_base_bits & !0x7F) | u16::from(val & 0x7F);
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
                        // Only mirroring/WRAM/CHR-RAM routing changes here; the
                        // routing is read per-access, so just refresh mirroring.
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
        self.sync(memory);
    }

    fn sync(&mut self, memory: &mut Memory) {
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

        // WRAM is up to 32K in 8K pages, optionally also visible at $4000.
        if self.wram_config_enabled {
            let bank = i32::from(self.wram_bank_select);
            memory.map_prg(0x4000, Self::WRAM_BANK, (bank + 1) & 0x03, Src::PrgRam);
            memory.map_prg(0x6000, Self::WRAM_BANK, bank, Src::PrgRam);
        } else {
            memory.unmap_prg(0x4000, Self::WRAM_BANK);
            if self.wram_enabled {
                memory.map_prg(0x6000, Self::WRAM_BANK, 0, Src::PrgRam);
            } else {
                memory.unmap_prg(0x6000, Self::WRAM_BANK);
            }
        }
        let writable = self.wram_writable();
        memory.set_prg_writable(0x4000, Self::WRAM_BANK, writable);
        memory.set_prg_writable(0x6000, Self::WRAM_BANK, writable);

        memory.set_mirroring(self.mirroring);
    }
}

impl Reset for Fk23C {
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

impl Clock for Fk23C {
    fn clock(&mut self) {
        self.mmc3.clock();
    }
}

impl Sram for Fk23C {}

impl Regional for Fk23C {}
