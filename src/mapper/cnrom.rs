//! CNROM (Mapper 3)
//!
//! [https://wiki.nesdev.com/w/index.php/CNROM]()
//! [https://wiki.nesdev.com/w/index.php/INES_Mapper_003]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::mapper::{Mapper, MapperRef, Mirroring};
use crate::memory::{Banks, Memory, Ram, Rom};
use crate::serialization::Savable;
use crate::Result;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

const PRG_ROM_BANK_SIZE: usize = 16 * 1024;
const CHR_ROM_BANK_SIZE: usize = 8 * 1024;

/// CNROM
#[derive(Debug)]
pub struct Cnrom {
    mirroring: Mirroring,
    prg_rom_bank_lo: usize,
    prg_rom_bank_hi: usize,
    chr_bank: usize,
    // CPU $8000-$FFFF 16 KB PRG ROM Bank 1 Fixed
    // CPU $C000-$FFFF 16 KB PRG ROM Bank 2 Fixed or Bank 1 Mirror if only 16 KB PRG ROM
    prg_rom_banks: Banks<Rom>,
    chr_banks: Banks<Ram>, // PPU $0000..=$1FFFF 8K CHR ROM Banks Switchable
}

impl Cnrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_rom_banks = Banks::init(&cart.prg_rom, PRG_ROM_BANK_SIZE);
        let chr_banks = Banks::init(&cart.chr_rom.to_ram(), CHR_ROM_BANK_SIZE);
        let cnrom = Self {
            mirroring: cart.mirroring(),
            prg_rom_bank_lo: 0usize,
            prg_rom_bank_hi: prg_rom_banks.len() - 1,
            chr_bank: 0usize,
            prg_rom_banks,
            chr_banks,
        };
        Rc::new(RefCell::new(cnrom))
    }
}

impl Mapper for Cnrom {
    fn irq_pending(&mut self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn vram_change(&mut self, _ppu: &Ppu, _addr: u16) {}
    fn clock(&mut self, _ppu: &Ppu) {} // no clocking
    fn battery_backed(&self) -> bool {
        false
    }
    fn save_sram(&self, _fh: &mut dyn Write) -> Result<()> {
        Ok(())
    }
    fn load_sram(&mut self, _fh: &mut dyn Read) -> Result<()> {
        Ok(())
    }
    fn chr(&self) -> Option<&Banks<Ram>> {
        Some(&self.chr_banks)
    }
    fn prg_rom(&self) -> Option<&Banks<Rom>> {
        Some(&self.prg_rom_banks)
    }
    fn prg_ram(&self) -> Option<&Ram> {
        None
    }
    fn set_logging(&mut self, _logging: bool) {}
}

impl Memory for Cnrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_banks[self.chr_bank].peek(addr),
            0x8000..=0xBFFF => self.prg_rom_banks[self.prg_rom_bank_lo].peek(addr - 0x8000),
            0xC000..=0xFFFF => self.prg_rom_banks[self.prg_rom_bank_hi].peek(addr - 0xC000),
            0x4020..=0x5FFF => 0, // Nothing at this range
            0x6000..=0x7FFF => 0, // No Save RAM
            _ => {
                eprintln!("unhandled Cnrom read at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x8000..=0xFFFF => self.chr_bank = val as usize & 3,
            0x0000..=0x1FFF => (), // ROM is write-only
            0x4020..=0x5FFF => (), // Nothing at this range
            0x6000..=0x7FFF => (), // No Save RAM
            _ => eprintln!("unhandled Cnrom write at address: 0x{:04X}", addr),
        }
    }

    fn reset(&mut self) {}
    fn power_cycle(&mut self) {}
}

impl Savable for Cnrom {
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        self.mirroring.save(fh)?;
        self.prg_rom_bank_lo.save(fh)?;
        self.prg_rom_bank_hi.save(fh)?;
        self.chr_bank.save(fh)?;
        self.prg_rom_banks.save(fh)?;
        self.chr_banks.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
        self.mirroring.load(fh)?;
        self.prg_rom_bank_lo.load(fh)?;
        self.prg_rom_bank_hi.load(fh)?;
        self.chr_bank.load(fh)?;
        self.prg_rom_banks.load(fh)?;
        self.chr_banks.load(fh)
    }
}
