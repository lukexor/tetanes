//! `CNROM` (Mapper 003).
//!
//! <https://wiki.nesdev.org/w/index.php/CNROM>
//! <https://wiki.nesdev.org/w/index.php/INES_Mapper_003>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// `CNROM` (Mapper 003).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Cnrom {
    pub chr_rom: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub mirroring: Mirroring,
    pub chr_banks: Banks,
    pub mirror_prg_rom: bool,
}

impl Cnrom {
    const CHR_ROM_WINDOW: usize = 8 * 1024;

    /// Load `Cnrom` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let chr_banks = Banks::new(0x0000, 0x1FFFF, chr_rom.len(), Self::CHR_ROM_WINDOW)?;
        let mirror_prg_rom = prg_rom.len() <= 0x4000;
        let cnrom = Self {
            chr_rom,
            prg_rom,
            mirroring: cart.mirroring(),
            chr_banks,
            mirror_prg_rom,
        };
        Ok(cnrom.into())
    }
}

impl Map for Cnrom {
    // PPU $0000..=$1FFF 8K CHR-ROM Banks Switchable
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Fixed
    // CPU $C000..=$FFFF 16K PRG-ROM Bank Fixed or Bank 1 Mirror if only 16 KB PRG-ROM

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
            0x8000..=0xBFFF => self.prg_rom[usize::from(addr & 0x3FFF)],
            0xC000..=0xFFFF => {
                let mirror = if self.mirror_prg_rom { 0x3FFF } else { 0x7FFF };
                self.prg_rom[usize::from(addr & mirror)]
            }
            _ => 0,
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        if let 0x8000..=0xFFFF = addr {
            self.chr_banks.set(0, val.into())
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Cnrom {}
impl Clock for Cnrom {}
impl Regional for Cnrom {}
impl Sram for Cnrom {}
