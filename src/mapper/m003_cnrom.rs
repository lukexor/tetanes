//! CNROM (Mapper 3)
//!
//! [https://wiki.nesdev.com/w/index.php/CNROM]()
//! [https://wiki.nesdev.com/w/index.php/INES_Mapper_003]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    mapper::{Mapper, MapperType, Mirroring},
    memory::{Banks, MemRead, MemWrite, Memory},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

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
    prg_rom_banks: Banks<Memory>,
    chr_banks: Banks<Memory>, // PPU $0000..=$1FFFF 8K CHR ROM Banks Switchable
    open_bus: u8,
}

impl Cnrom {
    pub fn load(cart: Cartridge) -> MapperType {
        let prg_rom_banks = Banks::init(&cart.prg_rom, PRG_ROM_BANK_SIZE);
        let chr_banks = Banks::init(&cart.chr_rom, CHR_ROM_BANK_SIZE);
        let cnrom = Self {
            mirroring: cart.mirroring(),
            prg_rom_bank_lo: 0usize,
            prg_rom_bank_hi: prg_rom_banks.len() - 1,
            chr_bank: 0usize,
            prg_rom_banks,
            chr_banks,
            open_bus: 0,
        };
        cnrom.into()
    }
}

impl Mapper for Cnrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Cnrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_banks[self.chr_bank].peek(addr),
            0x8000..=0xBFFF => self.prg_rom_banks[self.prg_rom_bank_lo].peek(addr - 0x8000),
            0xC000..=0xFFFF => self.prg_rom_banks[self.prg_rom_bank_hi].peek(addr - 0xC000),
            // 0x4020..=0x5FFF Nothing at this range
            // 0x6000..=0x7FFF No Save RAM
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Cnrom {
    fn write(&mut self, addr: u16, val: u8) {
        if let 0x8000..=0xFFFF = addr {
            self.chr_bank = val as usize & 3;
        }
        // 0x0000..=0x1FFF ROM is write-only
        // 0x4020..=0x5FFF Nothing at this range
        // 0x6000..=0x7FFF No Save RAM
    }
}

impl Clocked for Cnrom {}
impl Powered for Cnrom {}

impl Savable for Cnrom {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.mirroring.save(fh)?;
        self.prg_rom_bank_lo.save(fh)?;
        self.prg_rom_bank_hi.save(fh)?;
        self.chr_bank.save(fh)?;
        self.prg_rom_banks.save(fh)?;
        self.chr_banks.save(fh)?;
        self.open_bus.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.mirroring.load(fh)?;
        self.prg_rom_bank_lo.load(fh)?;
        self.prg_rom_bank_hi.load(fh)?;
        self.chr_bank.load(fh)?;
        self.prg_rom_banks.load(fh)?;
        self.chr_banks.load(fh)?;
        self.open_bus.load(fh)?;
        Ok(())
    }
}
