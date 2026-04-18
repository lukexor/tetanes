//! `Color Dreams` (Mapper 011).
//!
//! <https://wiki.nesdev.org/w/index.php/Color_Dreams>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper, Mirroring},
    mem::{Banks, Memory},
    ppu::CIRam,
};
use serde::{Deserialize, Serialize};

/// `Color Dreams` (Mapper 011).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct ColorDreams {
    pub chr_rom: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub mapper_num: u16,
    pub mirroring: Mirroring,
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl ColorDreams {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_ROM_WINDOW: usize = 8 * 1024;

    const CHR_BANK_MASK: u8 = 0b1111_0000;
    const PRG_BANK_MASK: u8 = 0b0000_0011;

    /// Load `ColorDreams` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr_rom.len(), Self::CHR_ROM_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_WINDOW)?;
        let color_dreams = Self {
            chr_rom,
            prg_rom,
            mapper_num: cart.mapper_num(),
            mirroring: cart.mirroring(),
            chr_banks,
            prg_rom_banks,
        };
        Ok(color_dreams.into())
    }
}

impl Map for ColorDreams {
    // PPU $0000..=$1FFF 8K switchable CHR-ROM bank
    // CPU $8000..=$FFFF 32K switchable PRG-ROM bank

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
    fn prg_write(&mut self, addr: u16, mut val: u8) {
        if let 0x8000..=0xFFFF = addr {
            if self.mapper_num == 144 {
                // Intentionally defective variant where only the least significant bit alwys wins
                // bus conflict
                // See: <https://www.nesdev.org/wiki/INES_Mapper_144>
                val |= self.prg_read(addr) & 0x01;
            }
            self.chr_banks
                .set(0, ((val & Self::CHR_BANK_MASK) >> 4).into());
            self.prg_rom_banks
                .set(0, (val & Self::PRG_BANK_MASK).into());
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for ColorDreams {}
impl Clock for ColorDreams {}
impl Regional for ColorDreams {}
impl Sram for ColorDreams {}
