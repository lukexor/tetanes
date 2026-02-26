//! `Bf909x` (Mapper 071).
//!
//! <https://wiki.nesdev.org/w/index.php?title=INES_Mapper_071>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// `Bf909x` revision.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    #[default]
    Bf909x,
    Bf9097,
}

/// `Bf909x` (Mapper 071).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bf909x {
    pub chr: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub has_chr_ram: bool,
    pub revision: Revision,
    pub mirroring: Mirroring,
    pub prg_rom_banks: Banks,
}

impl Bf909x {
    const PRG_ROM_WINDOW: usize = 16 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    const SINGLE_SCREEN_A: u8 = 0x10; // 0b10000

    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_ROM_WINDOW)?;
        let mut bf909x = Self {
            chr,
            prg_rom,
            has_chr_ram,
            revision: if cart.submapper_num() == 1 {
                Revision::Bf9097
            } else {
                Revision::Bf909x
            },
            mirroring: cart.mirroring(),
            prg_rom_banks,
        };
        bf909x.prg_rom_banks.set(1, bf909x.prg_rom_banks.last());
        Ok(bf909x.into())
    }

    pub const fn set_revision(&mut self, rev: Revision) {
        self.revision = rev;
    }
}

impl Map for Bf909x {
    // PPU $0000..=$1FFF 8K Fixed CHR-ROM Banks
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
            0x0000..=0x1FFF if self.has_chr_ram => self.chr[usize::from(addr)] = val,
            0x2000..=0x3EFF => ciram.write(addr, val, self.mirroring),
            _ => (),
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        if let 0x8000..=0xFFFF = addr {
            // Firehawk uses $9000 to change mirroring
            if addr == 0x9000 {
                self.revision = Revision::Bf9097;
            }
            if addr >= 0xC000 || self.revision != Revision::Bf9097 {
                self.prg_rom_banks.set(0, val.into());
            } else {
                self.mirroring = if val & Self::SINGLE_SCREEN_A == Self::SINGLE_SCREEN_A {
                    Mirroring::SingleScreenA
                } else {
                    Mirroring::SingleScreenB
                };
            }
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Bf909x {}
impl Clock for Bf909x {}
impl Regional for Bf909x {}
impl Sram for Bf909x {}
