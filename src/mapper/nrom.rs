use crate::cartridge::Cartridge;
use crate::mapper::Mirroring;
use crate::mapper::{Mapper, MapperRef};
use crate::memory::Memory;
use std::cell::RefCell;
use std::rc::Rc;

/// NROM (mapper 0)
///
/// http://wiki.nesdev.com/w/index.php/NROM
#[derive(Debug)]
pub struct Nrom {
    cart: Cartridge,
}

impl Nrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        Rc::new(RefCell::new(Self { cart }))
    }
}

impl Memory for Nrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU 8K Fixed CHR bank
            0x0000..=0x1FFF => {
                if self.cart.header.chr_rom_size == 0 {
                    self.cart.prg_ram[addr as usize]
                } else {
                    self.cart.chr_rom[addr as usize]
                }
            }
            0x6000..=0x7FFF => self.cart.prg_ram[(addr - 0x6000) as usize],
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
            0x0000..=0x1FFF => {
                if self.cart.header.chr_rom_size == 0 {
                    self.cart.prg_ram[addr as usize] = val;
                } else {
                    self.cart.chr_rom[addr as usize] = val;
                }
            }
            0x6000..=0x7FFF => self.cart.prg_ram[(addr - 0x6000) as usize] = val,
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

impl Mapper for Nrom {
    fn scanline_irq(&self) -> bool {
        false
    }
    fn step(&mut self) {
        // NOOP
    }
}
