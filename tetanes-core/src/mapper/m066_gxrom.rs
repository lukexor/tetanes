//! `GxROM` (Mapper 066).
//!
//! <https://wiki.nesdev.org/w/index.php?title=GxROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// `GxROM` (Mapper 066).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Gxrom {
    pub chr_rom: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub mirroring: Mirroring,
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl Gxrom {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    const CHR_BANK_MASK: u8 = 0x0F; // 0b1111
    const PRG_BANK_MASK: u8 = 0x30; // 0b110000

    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr_rom.len(), Self::CHR_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_ROM_WINDOW)?;
        let gxrom = Self {
            chr_rom,
            prg_rom,
            mirroring: cart.mirroring(),
            chr_banks,
            prg_rom_banks,
        };
        Ok(gxrom.into())
    }
}

impl Map for Gxrom {
    // PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable

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
        if let 0x8000..=0xFFFF = addr {
            self.chr_banks.set(0, (val & Self::CHR_BANK_MASK).into());
            self.prg_rom_banks
                .set(0, ((val & Self::PRG_BANK_MASK) >> 4).into());
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Gxrom {}
impl Clock for Gxrom {}
impl Regional for Gxrom {}
impl Sram for Gxrom {}
