//! `NINA-001` (Mapper 034).
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

/// `NINA-001` (Mapper 034).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nina001 {
    pub chr_rom: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub prg_ram: Memory<Box<[u8]>>,
    pub mirroring: Mirroring,
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl Nina001 {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024;
    const CHR_ROM_WINDOW: usize = 4 * 1024;

    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let prg_ram = Memory::with_ram_state(Self::PRG_RAM_SIZE, cart.ram_state);
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr_rom.len(), Self::CHR_ROM_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_ROM_WINDOW)?;
        let nina001 = Self {
            chr_rom,
            prg_rom,
            prg_ram,
            // hardwired to horizontal
            mirroring: Mirroring::Horizontal,
            chr_banks,
            prg_rom_banks,
        };
        Ok(nina001.into())
    }
}

impl Map for Nina001 {
    // PPU $0000..=$0FFF 4K switchable CHR ROM bank
    // PPU $1000..=$1FFF 4K switchable CHR ROM bank
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
            0x6000..=0x7FFF => self.prg_ram[usize::from(addr) & (Self::PRG_RAM_SIZE - 1)],
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => 0,
        }
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        match addr {
            0x0000..=0x1FFF => self.chr_rom[self.chr_banks.translate(addr)] = val,
            0x2000..=0x3EFF => ciram.write(addr, val, self.mirroring),
            _ => (),
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        if let 0x6000..=0x7FFF = addr {
            match addr {
                0x7FFD => self.prg_rom_banks.set(0, (val & 0x01).into()),
                0x7FFE => self.chr_banks.set(0, (val & 0x0F).into()),
                0x7FFF => self.chr_banks.set(1, (val & 0x0F).into()),
                _ => (),
            }
            self.prg_ram[usize::from(addr) & (Self::PRG_RAM_SIZE - 1)] = val;
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Nina001 {}
impl Clock for Nina001 {}
impl Regional for Nina001 {}
impl Sram for Nina001 {}
