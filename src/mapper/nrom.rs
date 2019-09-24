//! NROM (mapper 0)
//!
//! [http://wiki.nesdev.com/w/index.php/NROM]()

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
const PRG_RAM_SIZE: usize = 8 * 1024;
const CHR_RAM_SIZE: usize = 8 * 1024;

/// NROM
#[derive(Debug)]
pub struct Nrom {
    has_chr_ram: bool,
    battery_backed: bool,
    mirroring: Mirroring,
    open_bus: u8,
    nrom_size: NromSize,
    prg_ram: Ram, // CPU $6000-$7FFF 2K or 4K PRG RAM Family Basic only. 8K is provided
    // CPU $8000-$BFFF 16 KB PRG ROM Bank 1 for NROM128 or NROM256
    // CPU $C000-$FFFF 16 KB PRG ROM Bank 2 for NROM256 or Bank 1 Mirror for NROM128
    prg_rom_banks: Banks<Rom>,
    chr_banks: Banks<Ram>, // PPU $0000..=$1FFFF 8K Fixed CHR ROM Bank
}

#[derive(Debug, Copy, Clone)]
pub enum NromSize {
    Nrom128,
    Nrom256,
}
use NromSize::*;

impl Nrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_ram = Ram::init(PRG_RAM_SIZE);
        let prg_rom_banks = Banks::init(&cart.prg_rom, PRG_ROM_BANK_SIZE);
        let chr_banks = if cart.chr_rom.len() == 0 {
            let chr_ram = Ram::init(CHR_RAM_SIZE);
            Banks::init(&chr_ram, CHR_ROM_BANK_SIZE)
        } else {
            Banks::init(&cart.chr_rom.to_ram(), CHR_ROM_BANK_SIZE)
        };
        let nrom_size = if cart.prg_rom.len() > 0x4000 {
            Nrom256
        } else {
            Nrom128
        };
        let nrom = Self {
            has_chr_ram: cart.chr_rom.len() == 0,
            battery_backed: cart.battery_backed(),
            mirroring: cart.mirroring(),
            open_bus: 0u8,
            nrom_size,
            prg_ram,
            prg_rom_banks,
            chr_banks,
        };
        Rc::new(RefCell::new(nrom))
    }
}

impl Mapper for Nrom {
    fn irq_pending(&mut self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn vram_change(&mut self, _ppu: &Ppu, _addr: u16) {}
    fn clock(&mut self, _ppu: &Ppu) {} // No clocking
    fn battery_backed(&self) -> bool {
        self.battery_backed
    }
    fn save_sram(&self, fh: &mut dyn Write) -> Result<()> {
        if self.battery_backed {
            self.prg_ram.save(fh)?;
        }
        Ok(())
    }
    fn load_sram(&mut self, fh: &mut dyn Read) -> Result<()> {
        if self.battery_backed {
            self.prg_ram.load(fh)?;
        }
        Ok(())
    }
    fn chr(&self) -> Option<&Banks<Ram>> {
        Some(&self.chr_banks)
    }
    fn prg_rom(&self) -> Option<&Banks<Rom>> {
        Some(&self.prg_rom_banks)
    }
    fn prg_ram(&self) -> Option<&Ram> {
        Some(&self.prg_ram)
    }
    fn set_logging(&mut self, _logging: bool) {}
    fn nametable_mapping(&self, _addr: u16) -> bool {
        false
    }
}

impl Memory for Nrom {
    fn read(&mut self, addr: u16) -> u8 {
        let val = self.peek(addr);
        self.open_bus = val;
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            // PPU 8K Fixed CHR bank
            0x0000..=0x1FFF => self.chr_banks[0].peek(addr),
            0x6000..=0x7FFF => self.prg_ram.peek(addr - 0x6000),
            0x8000..=0xBFFF => self.prg_rom_banks[0].peek(addr & 0x3FFF),
            0xC000..=0xFFFF => match self.nrom_size {
                Nrom128 => self.prg_rom_banks[0].peek(addr & 0x3FFF),
                Nrom256 => self.prg_rom_banks[1].peek(addr & 0x7FFF),
            },
            0x4020..=0x5FFF => self.open_bus, // Nothing at this range
            _ => {
                eprintln!("invalid Nrom read at address: 0x{:04X}", addr);
                self.open_bus
            }
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        match addr {
            // Only CHR-RAM can be written to
            0x0000..=0x1FFF => {
                if self.has_chr_ram {
                    self.chr_banks[0].write(addr, val);
                }
            }
            0x6000..=0x7FFF => self.prg_ram.write(addr - 0x6000, val),
            0x4020..=0x5FFF => (), // Nothing at this range
            0x8000..=0xFFFF => (), // ROM is write-only
            _ => eprintln!(
                "invalid Nrom write at address: 0x{:04X} - val: 0x{:02X}",
                addr, val
            ),
        }
    }

    fn reset(&mut self) {}
    fn power_cycle(&mut self) {
        if self.battery_backed {
            self.prg_ram = Ram::init(self.prg_ram.len());
        }
        self.reset();
    }
}

impl Savable for Nrom {
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        self.has_chr_ram.save(fh)?;
        self.battery_backed.save(fh)?;
        self.mirroring.save(fh)?;
        self.nrom_size.save(fh)?;
        self.prg_ram.save(fh)?;
        self.prg_rom_banks.save(fh)?;
        self.chr_banks.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
        self.has_chr_ram.load(fh)?;
        self.battery_backed.load(fh)?;
        self.mirroring.load(fh)?;
        self.nrom_size.load(fh)?;
        self.prg_ram.load(fh)?;
        self.prg_rom_banks.load(fh)?;
        self.chr_banks.load(fh)
    }
}

impl Savable for NromSize {
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => NromSize::Nrom128,
            1 => NromSize::Nrom256,
            _ => panic!("invalid NromSize value"),
        };
        Ok(())
    }
}
