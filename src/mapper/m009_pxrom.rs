//! PxROM/MMC2 (mapper 9)
//!
//! [http://wiki.nesdev.com/w/index.php/MMC2]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    logging::Loggable,
    mapper::{Mapper, MapperType, Mirroring},
    memory::{Banks, MemRead, MemWrite, Memory},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

const PRG_ROM_BANK_SIZE: usize = 8 * 1024;
const CHR_ROM_BANK_SIZE: usize = 4 * 1024;
const PRG_RAM_SIZE: usize = 8 * 1024;

/// PxROM
#[derive(Debug)]
pub struct Pxrom {
    mirroring: Mirroring,
    // CHR ROM $FD/0000 bank select ($B000-$BFFF)
    // CHR ROM $FE/0000 bank select ($C000-$CFFF)
    // CHR ROM $FD/1000 bank select ($D000-$DFFF)
    // CHR ROM $FE/1000 bank select ($E000-$EFFF)
    // 7  bit  0
    // ---- ----
    // xxxC CCCC
    //    | ||||
    //    +-++++- Select 4 KB CHR ROM bank for PPU $0000/$1000-$0FFF/$1FFF
    //            used when latch 0/1 = $FD/$FE
    chr_rom_latch: [bool; 2], // Latch 0 and Latch 1
    prg_rom_bank_idx: [usize; 4],
    chr_rom_bank_idx: [usize; 4], // Banks for when Latches 0 and 1 are $FD or FE
    prg_ram: Memory,              // CPU $6000-$7FFF 8 KB PRG RAM bank (PlayChoice version only)
    // CPU $8000-$9FFF 8 KB switchable PRG ROM bank
    // CPU $A000-$FFFF Three 8 KB PRG ROM banks, fixed to the last three banks
    prg_rom_banks: Banks<Memory>,
    // PPU $0000..=$0FFFF Two 4 KB switchable CHR ROM banks
    // PPU $1000..=$1FFFF Two 4 KB switchable CHR ROM banks
    chr_banks: Banks<Memory>,
    open_bus: u8,
}

impl Pxrom {
    pub fn load(cart: Cartridge) -> MapperType {
        let prg_ram = Memory::ram(PRG_RAM_SIZE);
        let prg_rom_banks = Banks::init(&cart.prg_rom, PRG_ROM_BANK_SIZE);
        let chr_banks = Banks::init(&cart.chr_rom, CHR_ROM_BANK_SIZE);
        let prg_len = prg_rom_banks.len();
        let pxrom = Self {
            mirroring: cart.mirroring(),
            chr_rom_latch: [true; 2],
            prg_rom_bank_idx: [0, prg_len - 3, prg_len - 2, prg_len - 1],
            chr_rom_bank_idx: [0; 4],
            prg_ram,
            prg_rom_banks,
            chr_banks,
            open_bus: 0,
        };
        pxrom.into()
    }
}

impl Mapper for Pxrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Pxrom {
    fn read(&mut self, addr: u16) -> u8 {
        let val = self.peek(addr);
        match addr {
            0x0FD8 | 0x0FE8 | 0x1FD8..=0x1FDF | 0x1FE8..=0x1FEF => {
                // Sets latch 0 iff addr is either $0FD8 or $0FE8, 1 otherwise
                let latch = if addr & 0x1000 == 0 { 0 } else { 1 };
                // Sets true if addr is $-FE-
                self.chr_rom_latch[latch as usize] = ((addr & 0x0FF0) >> 4) == 0xFE;
            }
            _ => (),
        }
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x0FFF => {
                // Lo banks 0 and 1
                let idx = self.chr_rom_bank_idx[self.chr_rom_latch[0] as usize];
                self.chr_banks[idx].peek(addr)
            }
            0x1000..=0x1FFF => {
                // Hi banks 2 and 3
                let idx = self.chr_rom_bank_idx[2 + (self.chr_rom_latch[1] as usize)];
                self.chr_banks[idx].peek(addr - 0x1000)
            }
            0x6000..=0x7FFF => self.prg_ram.peek(addr - 0x6000),
            0x8000..=0x9FFF => self.prg_rom_banks[self.prg_rom_bank_idx[0]].peek(addr - 0x8000),
            0xA000..=0xFFFF => {
                let bank = (addr - 0x8000) as usize / PRG_ROM_BANK_SIZE;
                let addr = addr % PRG_ROM_BANK_SIZE as u16;
                self.prg_rom_banks[self.prg_rom_bank_idx[bank]].peek(addr)
            }
            // 0x4020..=0x5FFF Nothing at this range
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Pxrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x6000..=0x7FFF => self.prg_ram.write(addr - 0x6000, val),
            0xA000..=0xAFFF => self.prg_rom_bank_idx[0] = (val & 0x0F) as usize,
            0xB000..=0xBFFF => self.chr_rom_bank_idx[0] = (val & 0x1F) as usize,
            0xC000..=0xCFFF => self.chr_rom_bank_idx[1] = (val & 0x1F) as usize,
            0xD000..=0xDFFF => self.chr_rom_bank_idx[2] = (val & 0x1F) as usize,
            0xE000..=0xEFFF => self.chr_rom_bank_idx[3] = (val & 0x1F) as usize,
            0xF000..=0xFFFF => {
                self.mirroring = match val & 0x01 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    _ => panic!("impossible mirroring mode"),
                }
            }
            // 0x0000..=0x1FFF ROM is write-only
            // 0x4020..=0x5FFF Nothing at this range
            // 0x8000..=0x9FFF ROM is write-only
            _ => (),
        }
    }
}

impl Clocked for Pxrom {}

impl Powered for Pxrom {
    fn reset(&mut self) {
        self.chr_rom_latch = [true; 2];
    }
}

impl Loggable for Pxrom {}

impl Savable for Pxrom {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.mirroring.save(fh)?;
        self.chr_rom_latch.save(fh)?;
        self.prg_rom_bank_idx.save(fh)?;
        self.chr_rom_bank_idx.save(fh)?;
        self.prg_ram.save(fh)?;
        self.prg_rom_banks.save(fh)?;
        self.chr_banks.save(fh)?;
        self.open_bus.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.mirroring.load(fh)?;
        self.chr_rom_latch.load(fh)?;
        self.prg_rom_bank_idx.load(fh)?;
        self.chr_rom_bank_idx.load(fh)?;
        self.prg_ram.load(fh)?;
        self.prg_rom_banks.load(fh)?;
        self.chr_banks.load(fh)?;
        self.open_bus.load(fh)?;
        Ok(())
    }
}
