//! `SxROM`/`MMC1` (Mapper 001).
//!
//! <https://wiki.nesdev.org/w/index.php/SxROM>
//! <https://wiki.nesdev.org/w/index.php/MMC1>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sram},
    fs,
    mapper::{self, Map, Mapper, Mmc1, Mmc1Revision},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `SxROM`/`MMC1` (Mapper 001).
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sxrom {
    pub chr: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub prg_ram: Memory<Box<[u8]>>,
    pub chr_banks: Banks,
    pub prg_ram_banks: Banks,
    pub prg_rom_banks: Banks,
    pub mmc1: Mmc1,
    pub has_chr_ram: bool,
    pub submapper_num: u8,
    pub prg_select: bool,
}

impl Sxrom {
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const PRG_ROM_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;
    const PRG_RAM_SIZE: usize = 32 * 1024; // 32K is safely compatible sans NES 2.0 header
    const CHR_RAM_SIZE: usize = 8 * 1024;

    const PRG_BANK_MASK: u8 = 0x0F;
    const PRG_BANK_SELECT_MASK: u8 = 0x10; // 0b10000

    /// Load `Sxrom` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
        revision: Mmc1Revision,
    ) -> Result<Mapper, mapper::Error> {
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let prg_ram = cart.prg_ram_or_default(Self::PRG_RAM_SIZE);
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr.len(), Self::CHR_WINDOW)?;
        let prg_ram_banks = Banks::new(0x6000, 0x7FFF, prg_ram.len(), Self::PRG_RAM_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_ROM_WINDOW)?;
        let mut sxrom = Self {
            prg_rom,
            chr,
            prg_ram,
            chr_banks,
            prg_ram_banks,
            prg_rom_banks,
            mmc1: Mmc1::new(revision),
            has_chr_ram,
            submapper_num: cart.submapper_num(),
            prg_select: cart.prg_rom_size == 0x80000,
        };
        sxrom.update_state();
        Ok(sxrom.into())
    }

    /// Update internal state based on register flags.
    pub fn update_state(&mut self) {
        let extra_reg = if self.mmc1.last_chr_reg == 0xC000 && self.mmc1.chr_mode {
            self.mmc1.chr1
        } else {
            self.mmc1.chr0
        };
        let prg_bank_select = if self.prg_select {
            extra_reg & Self::PRG_BANK_SELECT_MASK
        } else {
            0x00
        };

        if self.submapper_num == 5 {
            // Fixed PRG SEROM, SHROM, SH1ROM use a fixed 32k PRG-ROM with no banking support.
            self.prg_rom_banks.set_range(0, 1, 0);
        } else if self.mmc1.prg_mode {
            if self.mmc1.prg_bank_select {
                self.prg_rom_banks
                    .set(0, (self.mmc1.prg | prg_bank_select).into());
                self.prg_rom_banks
                    .set(1, (Self::PRG_BANK_MASK | prg_bank_select).into());
            } else {
                self.prg_rom_banks.set(1, prg_bank_select.into());
                self.prg_rom_banks
                    .set(1, (self.mmc1.prg | prg_bank_select).into());
            }
        } else {
            self.prg_rom_banks
                .set_range(0, 1, ((self.mmc1.prg & 0xFE) | prg_bank_select).into()); // ignore low bit
        }

        if self.mmc1.chr_mode {
            self.chr_banks.set(0, self.mmc1.chr0.into());
            self.chr_banks.set(1, self.mmc1.chr1.into());
        } else {
            self.chr_banks.set(0, (self.mmc1.chr0 & 0x1E).into()); // ignore low bit
            self.chr_banks.set(1, ((self.mmc1.chr0 & 0x1E) + 1).into()); // ignore low bit
        }
    }
}

impl Map for Sxrom {
    // PPU $0000..=$1FFF 4K CHR-ROM/RAM Bank Switchable
    // CPU $6000..=$7FFF 8K PRG-RAM Bank (optional)
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable or Fixed to First Bank
    // CPU $C000..=$FFFF 16K PRG-ROM Bank Fixed to Last Bank or Switchable

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr[self.chr_banks.translate(addr)],
            0x2000..=0x3EFF => ciram.peek(addr, self.mirroring()),
            _ => 0,
        }
    }

    /// Peek a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_peek(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.mmc1.prg_ram_enabled() => {
                self.prg_ram[self.prg_ram_banks.translate(addr)]
            }
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => 0,
        }
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        match addr {
            0x0000..=0x1FFF if self.has_chr_ram => self.chr[self.chr_banks.translate(addr)] = val,
            0x2000..=0x3EFF => ciram.write(addr, val, self.mirroring()),
            _ => (),
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        match addr {
            0x6000..=0x7FFF if self.mmc1.prg_ram_enabled() => {
                self.prg_ram[self.prg_ram_banks.translate(addr)] = val;
            }
            0x8000..=0xFFFF => {
                let written = self.mmc1.process_shift_register_write(addr, val);
                if written {
                    self.update_state();
                }
            }
            _ => (),
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mmc1.mirroring
    }
}

impl Reset for Sxrom {
    fn reset(&mut self, kind: ResetKind) {
        self.mmc1.reset(kind);
        self.update_state();
    }
}

impl Clock for Sxrom {
    fn clock(&mut self) {
        self.mmc1.clock();
    }
}

impl Sram for Sxrom {
    /// Save RAM to a given path.
    fn save(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        fs::save(path.as_ref(), &self.prg_ram)
    }

    /// Load save RAM from a given path.
    fn load(&mut self, path: impl AsRef<Path>) -> fs::Result<()> {
        fs::load(path.as_ref()).map(|data: Memory<Box<[u8]>>| self.prg_ram = data)
    }
}

impl Regional for Sxrom {}

impl std::fmt::Debug for Sxrom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SxRom")
            .field("mmc1", &self.mmc1)
            .field("submapper_num", &self.submapper_num)
            .field("prg_select", &self.prg_select)
            .field("chr_banks", &self.chr_banks)
            .field("prg_ram_banks", &self.prg_ram_banks)
            .field("prg_rom_banks", &self.prg_rom_banks)
            .finish()
    }
}
