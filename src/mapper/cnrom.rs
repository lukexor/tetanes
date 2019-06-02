//! CNROM (Mapper 3)
//!
//! [https://wiki.nesdev.com/w/index.php/CNROM]()
//! [https://wiki.nesdev.com/w/index.php/INES_Mapper_003]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::mapper::{Mapper, MapperRef, Mirroring};
use crate::memory::Memory;
use crate::serialization::Savable;
use crate::util::Result;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

/// CNROM
#[derive(Debug)]
pub struct Cnrom {
    cart: Cartridge,
    chr_bank: u16, // $0000-$1FFF 8K CHR-ROM
    prg_bank_1: u16,
    prg_bank_2: u16,
}

impl Cnrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_bank_2 = (cart.header.prg_rom_size - 1) as u16;
        Rc::new(RefCell::new(Self {
            cart,
            chr_bank: 0u16,
            prg_bank_1: 0u16,
            prg_bank_2,
        }))
    }
}

impl Mapper for Cnrom {
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

impl Memory for Cnrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            // $0000-$1FFF PPU
            0x0000..=0x1FFF => {
                let addr = (self.chr_bank & self.cart.header.chr_rom_size - 1) * 0x2000 + addr;
                self.cart.chr[addr as usize]
            }
            0x6000..=0x7FFF => self.cart.prg_ram[(addr - 0x6000) as usize],
            // $8000-$FFFF CPU
            0x8000..=0xBFFF => {
                let addr = (self.prg_bank_1 & self.cart.header.prg_rom_size - 1) * 0x4000
                    + (addr - 0x8000);
                self.cart.prg_rom[addr as usize]
            }
            0xC000..=0xFFFF => {
                let addr = (self.prg_bank_2 & self.cart.header.prg_rom_size - 1) * 0x4000
                    + (addr - 0xC000);
                self.cart.prg_rom[addr as usize]
            }
            _ => {
                eprintln!("unhandled Cnrom readb at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => (), // ROM is read-only
            0x6000..=0x7FFF => self.cart.prg_ram[(addr - 0x6000) as usize] = val,
            // $8000-$FFFF CPU
            0x8000..=0xFFFF => self.chr_bank = u16::from(val & 3),
            _ => eprintln!("unhandled Cnrom writeb at address: 0x{:04X}", addr),
        }
    }
}

impl Savable for Cnrom {
    fn save(&self, fh: &mut Write) -> Result<()> {
        self.cart.save(fh)?;
        self.chr_bank.save(fh)?;
        self.prg_bank_1.save(fh)?;
        self.prg_bank_2.save(fh)
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        self.cart.load(fh)?;
        self.chr_bank.load(fh)?;
        self.prg_bank_1.load(fh)?;
        self.prg_bank_2.load(fh)
    }
}
