//! `NROM` (Mapper 000).
//!
//! <https://wiki.nesdev.org/w/index.php/NROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sram},
    mapper::{self, Map, Mapper},
    mem::{Memory, RamState},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// `NROM` (Mapper 000).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nrom {
    pub chr: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub prg_ram: Memory<Box<[u8]>>,
    pub mirroring: Mirroring,
    pub has_chr_ram: bool,
    pub mirror_prg_rom: bool,
    pub ram_state: RamState,
}

impl Nrom {
    const PRG_RAM_SIZE: usize = 8 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    /// Load `Nrom` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        // NROM doesn't have CHR-RAM - but a lot of homebrew games use Mapper 000 with CHR-RAM, so
        // we'll provide some if no CHR-ROM is available.
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let nrom = Self {
            chr,
            prg_rom,
            // Family Basic only supported 2-4K of PRG-RAM, but we'll provide 8K by default.
            prg_ram: Memory::with_ram_state(Self::PRG_RAM_SIZE, cart.ram_state),
            mirroring: cart.mirroring(),
            has_chr_ram,
            mirror_prg_rom: cart.prg_rom_size <= 0x4000,
            ram_state: cart.ram_state,
        };
        Ok(nrom.into())
    }
}

impl Map for Nrom {
    // PPU $0000..=$1FFF 8K Fixed CHR-ROM Bank
    // CPU $6000..=$7FFF 2K or 4K PRG-RAM Family Basic only. 8K is provided by default.
    // CPU $8000..=$BFFF 16K PRG-ROM Bank 1 for NROM128 or NROM256
    // CPU $C000..=$FFFF 16K PRG-ROM Bank 2 for NROM256 or Bank 1 Mirror for NROM128

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x2000..=0x3EFF => ciram.peek(addr, self.mirroring),
            0x0000..=0x1FFF => self.chr[usize::from(addr)],
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
            0x6000..=0x7FFF => self.prg_ram[usize::from(addr & 0x1FFF)],
            _ => 0,
        }
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        match addr {
            0x2000..=0x3EFF => ciram.write(addr, val, self.mirroring),
            0x0000..=0x1FFF if self.has_chr_ram => self.chr[usize::from(addr)] = val,
            _ => (),
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        if let 0x6000..=0x7FFF = addr {
            self.prg_ram[usize::from(addr & 0x1FFF)] = val;
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Nrom {
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            self.ram_state.fill(&mut self.prg_ram);
        }
    }
}

impl Clock for Nrom {}
impl Regional for Nrom {}
impl Sram for Nrom {}
