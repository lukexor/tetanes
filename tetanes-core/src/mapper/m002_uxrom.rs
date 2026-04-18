//! `UxROM` (Mapper 002).
//!
//! <https://wiki.nesdev.org/w/index.php/UxROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// `UxROM` (Mapper 002).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Uxrom {
    pub chr: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub has_chr_ram: bool,
    pub mirroring: Mirroring,
    pub prg_rom_banks: Banks,
}

impl Uxrom {
    const PRG_ROM_WINDOW: usize = 16 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    /// Load `Uxrom` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_ROM_WINDOW)?;
        let mut uxrom = Self {
            chr,
            prg_rom,
            has_chr_ram,
            mirroring: cart.mirroring(),
            prg_rom_banks,
        };
        uxrom.prg_rom_banks.set(1, uxrom.prg_rom_banks.last());
        Ok(uxrom.into())
    }
}

impl Map for Uxrom {
    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable
    // CPU $C000..=$FFFF 16K PRG-ROM Fixed to Last Bank

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr[usize::from(addr)],
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
            0x0000..=0x1FFF => self.chr[usize::from(addr)] = val,
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

impl Reset for Uxrom {}
impl Clock for Uxrom {}
impl Regional for Uxrom {}
impl Sram for Uxrom {}
