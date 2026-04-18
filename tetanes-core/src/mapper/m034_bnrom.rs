//! `BNROM` (Mapper 034).
//!
//! <https://www.nesdev.org/wiki/BNROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// `BNROM` (Mapper 034).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bnrom {
    pub chr: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub has_chr_ram: bool,
    pub mirroring: Mirroring,
    pub prg_rom_banks: Banks,
}

impl Bnrom {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    /// Load `Bnrom` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_ROM_WINDOW)?;
        let bnrom = Self {
            chr,
            prg_rom,
            has_chr_ram,
            mirroring: cart.mirroring(),
            prg_rom_banks,
        };
        Ok(bnrom.into())
    }
}

impl Map for Bnrom {
    // PPU $0000..=$1FFF 8K CHR-RAM Bank Fixed
    // CPU $8000..=$FFFF 32K switchable PRG-ROM bank

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr[usize::from(addr) & (Self::CHR_RAM_SIZE - 1)],
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

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        match addr {
            0x0000..=0x1FFF if self.has_chr_ram => self.chr[usize::from(addr)] = val,
            0x2000..=0x3EFF => ciram.write(addr, val, self.mirroring),
            _ => (),
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        if let 0x8000..=0xFFFF = addr {
            self.prg_rom_banks.set(0, val.into())
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Bnrom {}
impl Clock for Bnrom {}
impl Regional for Bnrom {}
impl Sram for Bnrom {}
