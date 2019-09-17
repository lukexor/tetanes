//! UxROM (Mapper 2)
//!
//! [https://wiki.nesdev.com/w/index.php/UxROM]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::mapper::{Mapper, MapperRef, Mirroring};
use crate::memory::{Banks, Memory, Ram, Rom};
use crate::serialization::Savable;
use crate::Result;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

const PRG_ROM_BANK_SIZE: usize = 16 * 1024; // 16L ROM
const CHR_BANK_SIZE: usize = 8 * 1024; // 8K ROM/RAM
const CHR_RAM_SIZE: usize = 8 * 1024;

/// UxROM
#[derive(Debug)]
pub struct Uxrom {
    mirroring: Mirroring,
    prg_rom_bank_lo: usize,
    prg_rom_bank_hi: usize, // prg_bank_hi is fixed to last bank
    // CPU $8000-$BFFF 16 KB PRG ROM Bank Switchable
    // CPU $C000-$FFFF 16 KB PRG ROM Fixed to Last Bank
    prg_rom_banks: Banks<Rom>,
    chr_banks: Banks<Ram>, // PPU $0000..=$1FFFF 8K Fixed CHR ROM Banks
}

impl Uxrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_rom_banks = Banks::init(&cart.prg_rom, PRG_ROM_BANK_SIZE);
        // Just 1 bank
        let chr_banks = if cart.chr_rom.len() == 0 {
            let chr_ram = Ram::init(CHR_RAM_SIZE);
            Banks::init(&chr_ram, CHR_BANK_SIZE)
        } else {
            Banks::init(&cart.chr_rom.to_ram(), CHR_BANK_SIZE)
        };
        let uxrom = Self {
            mirroring: cart.mirroring(),
            prg_rom_bank_lo: 0usize,
            prg_rom_bank_hi: prg_rom_banks.len() - 1,
            prg_rom_banks,
            chr_banks,
        };
        Rc::new(RefCell::new(uxrom))
    }
}

impl Mapper for Uxrom {
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
}

impl Memory for Uxrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_banks[0].peek(addr),
            0x8000..=0xBFFF => self.prg_rom_banks[self.prg_rom_bank_lo].peek(addr - 0x8000),
            0xC000..=0xFFFF => self.prg_rom_banks[self.prg_rom_bank_hi].peek(addr - 0xC000),
            0x6000..=0x7FFF => 0, // No Save RAM
            _ => {
                eprintln!("unhandled Uxrom read at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => self.chr_banks[0].write(addr, val),
            0x8000..=0xFFFF => self.prg_rom_bank_lo = (val as usize) % self.prg_rom_banks.len(),
            0x6000..=0x7FFF => (), // No Save RAM
            _ => {
                eprintln!(
                    "unhandled Sxrom write at address: 0x{:04X} - val: 0x{:02X}",
                    addr, val
                );
            }
        }
    }

    fn reset(&mut self) {}
    fn power_cycle(&mut self) {}
}

impl Savable for Uxrom {
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        self.mirroring.save(fh)?;
        self.prg_rom_bank_lo.save(fh)?;
        self.prg_rom_bank_hi.save(fh)?;
        self.prg_rom_banks.save(fh)?;
        self.chr_banks.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
        self.mirroring.load(fh)?;
        self.prg_rom_bank_lo.load(fh)?;
        self.prg_rom_bank_hi.load(fh)?;
        self.prg_rom_banks.load(fh)?;
        self.chr_banks.load(fh)
    }
}
