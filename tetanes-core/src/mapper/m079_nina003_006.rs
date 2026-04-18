//! `NINA-003`/`NINA-006` (Mapper 079).
//!
//! <https://www.nesdev.org/wiki/NINA-001>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// `NINA-003`/`NINA-006` (Mapper 079).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nina003006 {
    pub chr_rom: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub mirroring: Mirroring,
    pub mapper_num: u16,
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl Nina003006 {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const CHR_ROM_WINDOW: usize = 8 * 1024;

    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr_rom.len(), Self::CHR_ROM_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_ROM_WINDOW)?;
        let nina003006 = Self {
            chr_rom,
            prg_rom,
            mirroring: cart.mirroring(),
            mapper_num: cart.mapper_num(),
            chr_banks,
            prg_rom_banks,
        };
        Ok(nina003006.into())
    }
}

impl Map for Nina003006 {
    // PPU $0000..=$1FFF 8K switchable CHR ROM bank
    // CPU $8000..=$FFFF 32K switchable PRG ROM bank

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_rom[self.chr_banks.translate(addr)],
            0x2000..=0x3EFF => ciram.peek(addr, self.mirroring),
            _ => 0,
        }
    }

    /// Peek a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_peek(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => 0,
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        if (addr & 0xE100) == 0x4100 {
            if self.mapper_num == 113 {
                self.prg_rom_banks.set(0, ((val >> 3) & 0x07).into());
                self.chr_banks
                    .set(0, ((val & 0x07) | ((val >> 3) & 0x08)).into());
                self.mirroring = if val & 0x80 == 0x80 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
            } else {
                self.prg_rom_banks.set(0, ((val >> 3) & 0x01).into());
                self.chr_banks.set(0, (val & 0x07).into());
            }
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Nina003006 {}
impl Clock for Nina003006 {}
impl Regional for Nina003006 {}
impl Sram for Nina003006 {}
