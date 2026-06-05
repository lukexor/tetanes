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
    fs,
    mapper::{self, Map, Mapper, mmc3::Mmc3},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `Waixing FK23C`/`FS303` (Mapper 176).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Fk23C {
    /// CHR-ROM, or CHR-RAM for carts that ship no CHR-ROM (`has_chr_ram`).
    pub chr: Memory<Box<[u8]>>,
    /// 8KB CHR-RAM overlay for CHR-ROM carts: the RAM-config register can route
    /// bank values 0-7 here (custom-font tiles). Empty for CHR-RAM-only carts,
    /// whose RAM is `chr` itself.
    pub ext_vram: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    /// Up to 32KB WRAM, banked in 8KB pages by `$A001`.
    pub prg_ram: Memory<Box<[u8]>>,
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
    /// Power-on `prg_base_bits`: the 1MB-PRG subtype-1 boot quirk selects the
    /// upper 512KB; everything else boots at 0.
    pub init_prg_base: u16,
    pub mirroring: Mirroring,
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl Fk23C {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    const WRAM_SIZE: usize = 32 * 1024;
    const WRAM_BANK: usize = 8 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;
    const EXT_VRAM_SIZE: usize = 8 * 1024;

    /// Standard MMC3 registers `$0`-`$7` plus extended `$8`-`$B` power-on values.
    const INIT_REGS: [u8; 12] = [0, 2, 4, 5, 6, 7, 0, 1, 0xFE, 0xFF, 0xFF, 0xFF];

    /// Load `Fk23C` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let prg_ram = Memory::with_ram_state(Self::WRAM_SIZE, cart.ram_state);
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr.len(), Self::CHR_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_WINDOW)?;
        // Subtype 1 (1MB PRG-ROM == 1MB CHR-ROM) boots in the upper 512KB.
        let init_prg_base = if prg_rom.len() == 1024 * 1024 && prg_rom.len() == cart.chr_rom_size {
            0x20
        } else {
            0
        };
        let mut fk23c = Self {
            // The CHR-RAM overlay only applies to CHR-ROM carts; a CHR-RAM-only
            // cart's `chr` is the RAM itself.
            ext_vram: if has_chr_ram {
                Memory::empty()
            } else {
                Memory::new(Self::EXT_VRAM_SIZE)
            },
            chr,
            prg_rom,
            prg_ram,
            mmc3: Mmc3::default(),
            // Register state below is set by reset(); these are placeholders.
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
            init_prg_base,
            mirroring: cart.mirroring(),
            chr_banks,
            prg_rom_banks,
        };
        fk23c.reset(ResetKind::Hard);
        Ok(fk23c.into())
    }

    /// Whether the CHR bank covering `addr` reads/writes the 8KB CHR-RAM overlay
    /// rather than CHR-ROM. The RAM-config register routes bank values 0-7 to
    /// RAM (custom fonts); `$5xx0.5` forces all CHR to RAM. Always false for
    /// CHR-RAM-only carts (whose `ext_vram` is empty and whose `chr` is RAM).
    fn chr_uses_ext_vram(&self, addr: u16) -> bool {
        if self.ext_vram.is_empty() {
            return false;
        }
        if self.select_chr_ram {
            return true;
        }
        // Only the first 8KB (bank values 0-7) route to RAM.
        self.wram_config_enabled
            && self.ram_in_first_chr_bank
            && self.chr_banks.page(self.chr_banks.get(addr)) <= 7
    }

    fn update_prg(&mut self) {
        match self.prg_banking_mode {
            0..=2 => {
                // invert_prg_a14 swaps the $8000 and $C000 banks (slots 0 and 2).
                let swap = if self.invert_prg_a14 { 2 } else { 0 };
                if self.extended_mmc3_mode {
                    let outer = (self.prg_base_bits as usize) << 1;
                    self.prg_rom_banks
                        .set(swap, self.mmc3.bank_values[6] as usize | outer);
                    self.prg_rom_banks
                        .set(1, self.mmc3.bank_values[7] as usize | outer);
                    self.prg_rom_banks
                        .set(2 ^ swap, self.bank_values_ext[0] as usize | outer);
                    self.prg_rom_banks
                        .set(3, self.bank_values_ext[1] as usize | outer);
                } else {
                    let inner_mask = 0x3Fusize >> self.prg_banking_mode;
                    let outer = ((self.prg_base_bits as usize) << 1) & !inner_mask;
                    let r6 = self.mmc3.bank_values[6] as usize;
                    let r7 = self.mmc3.bank_values[7] as usize;
                    self.prg_rom_banks.set(swap, (r6 & inner_mask) | outer);
                    self.prg_rom_banks.set(1, (r7 & inner_mask) | outer);
                    self.prg_rom_banks
                        .set(2 ^ swap, (0xFE & inner_mask) | outer);
                    self.prg_rom_banks.set(3, (0xFF & inner_mask) | outer);
                }
            }
            3 => {
                // NROM-128: 16KB mirrored.
                let bank = (self.prg_base_bits as usize) << 1;
                self.prg_rom_banks.set(0, bank);
                self.prg_rom_banks.set(1, bank | 1);
                self.prg_rom_banks.set(2, bank);
                self.prg_rom_banks.set(3, bank | 1);
            }
            4 => {
                // NROM-256: 32KB.
                let bank = ((self.prg_base_bits as usize) & 0xFFE) << 1;
                self.prg_rom_banks.set(0, bank);
                self.prg_rom_banks.set(1, bank | 1);
                self.prg_rom_banks.set(2, bank | 2);
                self.prg_rom_banks.set(3, bank | 3);
            }
            _ => {}
        }
    }

    fn update_chr(&mut self) {
        let swap = if self.invert_chr_a12 { 0x04 } else { 0 };
        if !self.mmc3_chr_mode {
            let inner_mask = if self.cnrom_chr_mode {
                if self.outer_chr_bank_size { 1 } else { 3 }
            } else {
                0
            };
            for i in 0..8usize {
                let page = (((self.cnrom_chr_reg & inner_mask) as usize
                    | self.chr_base_bits as usize)
                    << 3)
                    + i;
                self.chr_banks.set(i, page);
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
                self.chr_banks.set(slot ^ swap, page | outer);
            }
        } else {
            let inner_mask = if self.outer_chr_bank_size { 0x7F } else { 0xFF };
            let outer = ((self.chr_base_bits as usize) << 3) & !inner_mask;
            let bv = self.mmc3.bank_values;
            let pages = [
                ((bv[0] & 0xFE) as usize & inner_mask) | outer,
                ((bv[0] | 0x01) as usize & inner_mask) | outer,
                ((bv[1] & 0xFE) as usize & inner_mask) | outer,
                ((bv[1] | 0x01) as usize & inner_mask) | outer,
                (bv[2] as usize & inner_mask) | outer,
                (bv[3] as usize & inner_mask) | outer,
                (bv[4] as usize & inner_mask) | outer,
                (bv[5] as usize & inner_mask) | outer,
            ];
            for (slot, page) in pages.into_iter().enumerate() {
                self.chr_banks.set(slot ^ swap, page);
            }
        }
    }

    /// Recompute only the mirroring mode. `$A000`/`$A001` change mirroring but
    /// not PRG/CHR banking, so they skip the bank rebank.
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

    fn update_state(&mut self) {
        self.update_mirroring();
        self.update_prg();
        self.update_chr();
    }

    /// Offset into WRAM for a CPU address, or `None` when no WRAM is mapped
    /// there (open bus). `$A001` selects the 8KB bank and enables/disables WRAM.
    fn wram_offset(&self, addr: u16) -> Option<usize> {
        match addr {
            0x4000..=0x5FFF if self.wram_config_enabled => {
                let bank = (self.wram_bank_select as usize + 1) & 0x03;
                Some(bank * Self::WRAM_BANK + usize::from(addr - 0x4000))
            }
            0x6000..=0x7FFF => {
                if self.wram_config_enabled {
                    Some(
                        self.wram_bank_select as usize * Self::WRAM_BANK
                            + usize::from(addr - 0x6000),
                    )
                } else if self.wram_enabled {
                    Some(usize::from(addr - 0x6000))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Whether WRAM writes are allowed. The `$A001` write-protect bit only
    /// applies in the non-RAM-config mode; RAM-config banking is always R/W.
    const fn wram_writable(&self) -> bool {
        self.wram_config_enabled || !self.wram_write_protected
    }
}

impl Map for Fk23C {
    #[inline(always)]
    fn chr_read(&mut self, addr: u16, ciram: &CIRam) -> u8 {
        self.ppu_read(addr);
        self.chr_peek(addr, ciram)
    }

    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let off = self.chr_banks.translate(addr);
                if self.chr_uses_ext_vram(addr) {
                    self.ext_vram[off]
                } else {
                    self.chr[off]
                }
            }
            0x2000..=0x3EFF => ciram.peek(addr, self.mirroring),
            _ => 0,
        }
    }

    #[inline(always)]
    fn prg_peek(&self, addr: u16) -> u8 {
        match addr {
            0x4000..=0x7FFF => self.wram_offset(addr).map_or(0, |off| self.prg_ram[off]),
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => 0,
        }
    }

    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        match addr {
            0x0000..=0x1FFF => {
                let off = self.chr_banks.translate(addr);
                if self.chr_uses_ext_vram(addr) {
                    self.ext_vram[off] = val;
                } else if self.has_chr_ram {
                    // CHR-RAM-only cart: `chr` is writable RAM.
                    self.chr[off] = val;
                }
                // Otherwise CHR-ROM: read-only, ignore.
            }
            0x2000..=0x3EFF => ciram.write(addr, val, self.mirroring),
            _ => (),
        }
    }

    fn prg_write(&mut self, addr: u16, val: u8) {
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
                } else if self.wram_writable() {
                    if let Some(off) = self.wram_offset(addr) {
                        self.prg_ram[off] = val;
                    }
                }
            }
            0x6000..=0x7FFF if self.wram_writable() => {
                if let Some(off) = self.wram_offset(addr) {
                    self.prg_ram[off] = val;
                }
            }
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
    }

    fn ppu_read(&mut self, addr: u16) {
        // FK23C clones use the standard (MMC3B/C) scanline IRQ (mmc3 defaults to BC).
        self.mmc3.clock_irq(addr);
    }

    fn irq_pending(&self) -> bool {
        self.mmc3.irq_pending()
    }

    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
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

impl Sram for Fk23C {
    fn save(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        fs::save(path.as_ref(), &self.prg_ram)
    }

    fn load(&mut self, path: impl AsRef<Path>) -> fs::Result<()> {
        fs::load(path.as_ref()).map(|data: Memory<Box<[u8]>>| self.prg_ram = data)
    }
}

impl Regional for Fk23C {}
