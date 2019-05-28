//! NROM (mapper 0)
//!
//! [http://wiki.nesdev.com/w/index.php/NROM]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::mapper::{Mapper, MapperRef, Mirroring};
use crate::memory::Memory;
use crate::serialization::Savable;
use crate::util::Result;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

/// NROM
#[derive(Debug)]
pub struct Nrom {
    cart: Cartridge,
}

impl Nrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        Rc::new(RefCell::new(Self { cart }))
    }
}

impl Mapper for Nrom {
    fn irq_pending(&self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        match self.cart.header.flags & 0x01 {
            0 => Mirroring::Horizontal,
            1 => Mirroring::Vertical,
            _ => panic!("invalid mirroring"),
        }
    }
    fn step(&mut self, _ppu: &Ppu) {}
    fn cart(&self) -> &Cartridge {
        &self.cart
    }
    fn cart_mut(&mut self) -> &mut Cartridge {
        &mut self.cart
    }
}

impl Memory for Nrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU 8K Fixed CHR bank
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize],
            0x6000..=0x7FFF => self.cart.sram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => {
                // CPU 32K Fixed PRG ROM bank for NROM-256
                if self.cart.prg_rom.len() > 0x4000 {
                    self.cart.prg_rom[(addr & 0x7FFF) as usize]
                // CPU 16K Fixed PRG ROM bank for NROM-128
                } else {
                    self.cart.prg_rom[(addr & 0x3FFF) as usize]
                }
            }
            _ => {
                eprintln!("unhandled Nrom readb at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => self.cart.chr_rom[addr as usize] = val,
            0x6000..=0x7FFF => self.cart.sram[(addr - 0x6000) as usize] = val,
            0x8000..=0xFFFF => {
                // CPU 32K Fixed PRG ROM bank for NROM-256
                if self.cart.prg_rom.len() > 0x4000 {
                    self.cart.prg_rom[(addr & 0x7FFF) as usize] = val;
                // CPU 16K Fixed PRG ROM bank for NROM-128
                } else {
                    self.cart.prg_rom[(addr & 0x3FFF) as usize] = val;
                }
            }
            _ => eprintln!(
                "invalid Nrom writeb at address: 0x{:04X} - val: 0x{:02X}",
                addr, val
            ),
        }
    }
}

impl Savable for Nrom {
    fn save(&self, _fh: &mut Write) -> Result<()> {
        Ok(())
    }
    fn load(&mut self, _fh: &mut Read) -> Result<()> {
        Ok(())
    }
}
