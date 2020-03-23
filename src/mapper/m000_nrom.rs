//! NROM (mapper 0)
//!
//! [http://wiki.nesdev.com/w/index.php/NROM]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    logging::Loggable,
    mapper::{Mapper, MapperRef, Mirroring},
    memory::{Banks, MemRead, MemWrite, Memory},
    serialization::Savable,
    NesResult,
};
use std::{
    cell::RefCell,
    io::{Read, Write},
    rc::Rc,
};

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
    prg_ram: Memory, // CPU $6000-$7FFF 2K or 4K PRG RAM Family Basic only. 8K is provided
    // CPU $8000-$BFFF 16 KB PRG ROM Bank 1 for NROM128 or NROM256
    // CPU $C000-$FFFF 16 KB PRG ROM Bank 2 for NROM256 or Bank 1 Mirror for NROM128
    prg_rom_banks: Banks<Memory>,
    chr_banks: Banks<Memory>, // PPU $0000..=$1FFFF 8K Fixed CHR ROM Bank
}

#[derive(Debug, Copy, Clone)]
pub enum NromSize {
    Nrom128,
    Nrom256,
}
use NromSize::*;

impl Nrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_ram = Memory::ram(PRG_RAM_SIZE);
        let prg_rom_banks = Banks::init(&cart.prg_rom, PRG_ROM_BANK_SIZE);
        let chr_banks = if cart.chr_rom.is_empty() {
            let chr_ram = Memory::ram(CHR_RAM_SIZE);
            Banks::init(&chr_ram, CHR_ROM_BANK_SIZE)
        } else {
            Banks::init(&cart.chr_rom, CHR_ROM_BANK_SIZE)
        };
        let nrom_size = if cart.prg_rom.len() > 0x4000 {
            Nrom256
        } else {
            Nrom128
        };
        let nrom = Self {
            has_chr_ram: cart.chr_rom.is_empty(),
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
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn battery_backed(&self) -> bool {
        self.battery_backed
    }
    fn save_sram(&self, fh: &mut dyn Write) -> NesResult<()> {
        if self.battery_backed {
            self.prg_ram.save(fh)?;
        }
        Ok(())
    }
    fn load_sram(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        if self.battery_backed {
            self.prg_ram.load(fh)?;
        }
        Ok(())
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Nrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
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
            // 0x4020..=0x5FFF Nothing at this range
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Nrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            // Only CHR-RAM can be written to
            0x0000..=0x1FFF if self.has_chr_ram => self.chr_banks[0].write(addr, val),
            0x6000..=0x7FFF => self.prg_ram.write(addr - 0x6000, val),
            // 0x4020..=0x5FFF Nothing at this range
            // 0x8000..=0xFFFF ROM is write-only
            _ => (),
        }
    }
}

impl Clocked for Nrom {}
impl Powered for Nrom {}
impl Loggable for Nrom {}

impl Savable for Nrom {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.has_chr_ram.save(fh)?;
        self.battery_backed.save(fh)?;
        self.mirroring.save(fh)?;
        self.open_bus.save(fh)?;
        self.nrom_size.save(fh)?;
        self.prg_ram.save(fh)?;
        self.prg_rom_banks.save(fh)?;
        self.chr_banks.save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.has_chr_ram.load(fh)?;
        self.battery_backed.load(fh)?;
        self.mirroring.load(fh)?;
        self.open_bus.load(fh)?;
        self.nrom_size.load(fh)?;
        self.prg_ram.load(fh)?;
        self.prg_rom_banks.load(fh)?;
        self.chr_banks.load(fh)?;
        Ok(())
    }
}

impl Savable for NromSize {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
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
