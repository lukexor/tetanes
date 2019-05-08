use super::memory::{Addr, Byte, Memory, Rom};
use crate::console::cartridge::{Board, Cartridge, ScanlineIrqResult};
use std::{error::Error, fmt};

/// Nrom Board (mapper 0)
///
/// http://wiki.nesdev.com/w/index.php/NROM
pub struct Nrom {
    prg_rom: Rom,
    chr_rom: Rom,
}

impl Nrom {
    pub fn load(cart: &mut Cartridge) -> Self {
        Self {
            prg_rom: Rom::with_bytes(cart.prg_rom.bytes.clone()),
            chr_rom: Rom::with_bytes(cart.chr_rom.bytes.clone()),
        }
    }
}

impl Memory for Nrom {
    fn readb(&self, addr: Addr) -> Byte {
        match addr {
            0x0000...0x1FFF => self.chr_rom.readb(addr & 0x1FFF),
            _ => {
                if self.prg_rom.len() > 0x4000 {
                    self.prg_rom.readb(addr & 0x7FFF)
                } else {
                    self.prg_rom.readb(addr & 0x3FFF)
                }
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            _ => eprintln!("unhandled Nrom writeb at address: 0x{:04X}", addr),
        }
    }
}

impl Board for Nrom {
    fn scanline_irq(&self) -> ScanlineIrqResult {
        ScanlineIrqResult::Continue
    }
}

impl fmt::Debug for Nrom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Nrom {{ PRG-ROM: {}KB, CHR-RAM: {}KB }}",
            self.prg_rom.len() / 0x0400,
            self.chr_rom.len() / 0x0400,
        )
    }
}

/// SxRom

pub struct Sxrom;

impl Sxrom {
    pub fn load(cart: &mut Cartridge) -> Self {
        Self {}
    }
}

impl Board for Sxrom {
    fn scanline_irq(&self) -> ScanlineIrqResult {
        ScanlineIrqResult::Continue
    }
}

impl Memory for Sxrom {
    fn readb(&self, addr: u16) -> u8 {
        0
    }

    fn writeb(&mut self, addr: u16, val: u8) {}
}

impl fmt::Debug for Sxrom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Sxrom {{ }}",)
    }
}
