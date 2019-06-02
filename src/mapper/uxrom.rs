//! UxRom (Mapper 2)
//!
//! [https://wiki.nesdev.com/w/index.php/UxROM]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::mapper::{Mapper, MapperRef, Mirroring};
use crate::memory::Memory;
use crate::serialization::Savable;
use crate::util::Result;
use std::cell::RefCell;
use std::fmt;
use std::io::{Read, Write};
use std::rc::Rc;

/// UXROM
pub struct Uxrom {
    cart: Cartridge,
    prg_banks: u8,
    prg_bank1: u8,
    prg_bank2: u8,
}

impl Uxrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_banks = (cart.prg_rom.len() / 0x4000) as u8;
        let uxrom = Self {
            cart,
            prg_banks,
            prg_bank1: 0u8,
            prg_bank2: prg_banks - 1,
        };
        Rc::new(RefCell::new(uxrom))
    }
}

impl Mapper for Uxrom {
    fn irq_pending(&mut self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        match self.cart.header.flags & 0x01 {
            0 => Mirroring::Horizontal,
            1 => Mirroring::Vertical,
            _ => panic!("invalid mirroring"),
        }
    }
    fn clock(&mut self, _ppu: &Ppu) {}
    fn cart(&self) -> &Cartridge {
        &self.cart
    }
    fn cart_mut(&mut self) -> &mut Cartridge {
        &mut self.cart
    }
}

impl Memory for Uxrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cart.chr[addr as usize],
            0x6000..=0x7FFF => self.cart.sram[(addr - 0x6000) as usize],
            0x8000..=0xBFFF => {
                let idx = u32::from(self.prg_bank1) * 0x4000 + u32::from(addr - 0x8000);
                self.cart.prg_rom[idx as usize]
            }
            0xC000..=0xFFFF => {
                let idx = u32::from(self.prg_bank2) * 0x4000 + u32::from(addr - 0xC000);
                self.cart.prg_rom[idx as usize]
            }
            _ => {
                eprintln!("unhandled Uxrom readb at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => self.cart.chr[addr as usize] = val,
            0x6000..=0x7FFF => self.cart.sram[(addr - 0x6000) as usize] = val,
            0x8000..=0xFFFF => {
                self.prg_bank1 = val % self.prg_banks;
            }
            _ => {
                eprintln!(
                    "unhandled Sxrom writeb at address: 0x{:04X} - val: 0x{:02X}",
                    addr, val
                );
            }
        }
    }
}

impl Savable for Uxrom {
    fn save(&self, fh: &mut Write) -> Result<()> {
        self.prg_banks.save(fh)?;
        self.prg_bank1.save(fh)?;
        self.prg_bank2.save(fh)
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        self.prg_banks.load(fh)?;
        self.prg_bank1.load(fh)?;
        self.prg_bank2.load(fh)
    }
}

impl fmt::Debug for Uxrom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Uxrom {{ cart: {:?}, mirroring: {:?} }}",
            self.cart,
            self.mirroring()
        )
    }
}
