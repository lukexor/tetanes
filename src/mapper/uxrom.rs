//! UxROM (Mapper 2)
//!
//! [https://wiki.nesdev.com/w/index.php/UxROM]()

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
    prg_rom_banks: Banks<Memory>,
    chr_banks: Banks<Memory>, // PPU $0000..=$1FFFF 8K Fixed CHR ROM Banks
    open_bus: u8,
}

impl Uxrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_rom_banks = Banks::init(&cart.prg_rom, PRG_ROM_BANK_SIZE);
        // Just 1 bank
        let chr_banks = if cart.chr_rom.is_empty() {
            let chr_ram = Memory::ram(CHR_RAM_SIZE);
            Banks::init(&chr_ram, CHR_BANK_SIZE)
        } else {
            Banks::init(&cart.chr_rom, CHR_BANK_SIZE)
        };
        let uxrom = Self {
            mirroring: cart.mirroring(),
            prg_rom_bank_lo: 0usize,
            prg_rom_bank_hi: prg_rom_banks.len() - 1,
            prg_rom_banks,
            chr_banks,
            open_bus: 0,
        };
        Rc::new(RefCell::new(uxrom))
    }
}

impl Mapper for Uxrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Uxrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_banks[0].peek(addr),
            0x8000..=0xBFFF => self.prg_rom_banks[self.prg_rom_bank_lo].peek(addr - 0x8000),
            0xC000..=0xFFFF => self.prg_rom_banks[self.prg_rom_bank_hi].peek(addr - 0xC000),
            // 0x4020..=0x5FFF Nothing at this range
            // 0x6000..=0x7FFF No Save RAM
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Uxrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => self.chr_banks[0].write(addr, val),
            0x8000..=0xFFFF => self.prg_rom_bank_lo = (val as usize) % self.prg_rom_banks.len(),
            // 0x4020..=0x5FFF // Nothing at this range
            // 0x6000..=0x7FFF // No Save RAM
            _ => (),
        }
    }
}

impl Clocked for Uxrom {}
impl Powered for Uxrom {}
impl Loggable for Uxrom {}

impl Savable for Uxrom {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.mirroring.save(fh)?;
        self.prg_rom_bank_lo.save(fh)?;
        self.prg_rom_bank_hi.save(fh)?;
        self.prg_rom_banks.save(fh)?;
        self.chr_banks.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.mirroring.load(fh)?;
        self.prg_rom_bank_lo.load(fh)?;
        self.prg_rom_bank_hi.load(fh)?;
        self.prg_rom_banks.load(fh)?;
        self.chr_banks.load(fh)
    }
}
