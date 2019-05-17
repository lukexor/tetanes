//! NES Board Mappers
//!
//! http://wiki.nesdev.com/w/index.php/Mapper

use crate::console::cartridge::{Board, Cartridge, Mirroring, PRG_BANK_SIZE};
use crate::console::memory::Memory;
use std::fmt;

/// NROM Board (mapper 0)
///
/// http://wiki.nesdev.com/w/index.php/NROM
#[derive(Debug)]
pub struct Nrom {
    cart: Cartridge,
}

impl Nrom {
    pub fn load(cart: Cartridge) -> Self {
        Self { cart }
    }
}

impl Memory for Nrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU 8K Fixed CHR bank
            0x0000..=0x1FFF => {
                if self.cart.num_chr_banks == 0 {
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
                if self.cart.num_chr_banks == 0 {
                    self.cart.prg_ram[addr as usize] = val;
                } else {
                    // self.cart.chr_rom[addr as usize] = val;
                }
            }
            0x6000..=0x7FFF => self.cart.prg_ram[(addr - 0x6000) as usize] = val,
            0x8000..=0xFFFF => {
                // CPU 32K Fixed PRG ROM bank for NROM-256
                if self.cart.prg_rom.len() > 0x4000 {
                    // self.cart.prg_rom[(addr & 0x7FFF) as usize] = val;
                    // CPU 16K Fixed PRG ROM bank for NROM-128
                } else {
                    // self.cart.prg_rom[(addr & 0x3FFF) as usize] = val;
                }
            }
            _ => eprintln!(
                "invalid Nrom writeb at address: 0x{:04X} - val: 0x{:02X}",
                addr, val
            ),
        }
    }
}

impl Board for Nrom {
    fn scanline_irq(&self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        self.cart.mirroring
    }
    fn step(&mut self) {
        // NOOP
    }
}

/// SxRom (Mapper 1/MMC1)
///
/// http://wiki.nesdev.com/w/index.php/SxROM
/// http://wiki.nesdev.com/w/index.php/MMC1

pub struct Sxrom {
    cart: Cartridge,
    // Registers
    ctrl: u8,                // $8000-$9FFF
    chr_bank_0: u8,          // $A000-$BFFF
    chr_bank_1: u8,          // $C000-$DFFF
    prg_bank: u8,            // $E000-$FFFF
    shift_register: u8,      // Write every 5th write
    prg_ram: [u8; 8 * 1024], // 8KB
    chr_ram: [u8; 8 * 1024], // 8KB
}

enum SxMirroring {
    OneScreenLower,
    OneScreenUpper,
    Vertical,
    Horizontal,
}

#[derive(Debug)]
enum SxPrgBankMode {
    Switch32,
    FixFirst,
    FixLast,
}

use SxPrgBankMode::*;

enum SxChrBankMode {
    Switch8,
    Switch4,
}

use SxChrBankMode::*;

impl Sxrom {
    pub fn load(cart: Cartridge) -> Self {
        Self {
            cart,
            ctrl: 0x0C,
            chr_bank_0: 0u8,
            chr_bank_1: 0u8,
            prg_bank: 0u8,
            shift_register: 0x10,
            prg_ram: [0u8; 8 * 1024], // 8KB
            chr_ram: [0u8; 8 * 1024], // 8KB
        }
    }

    fn prg_rom_bank_mode(&self) -> SxPrgBankMode {
        match (self.ctrl >> 2) & 3 {
            0 | 1 => Switch32,
            2 => FixFirst,
            3 => FixLast,
            _ => panic!("invalid prg bank mode"),
        }
    }

    fn get_prg_rom_bank(&self, addr: u16) -> u16 {
        let prg_mode = self.prg_rom_bank_mode();
        let bank = if addr < 0xC000 {
            match prg_mode {
                Switch32 => self.prg_bank & 0xFE, // Switch 32k, ignore low bit of bank number
                FixFirst => 0,                    // Fix first bank here, switch 16K at 0xC000
                FixLast => self.prg_bank,         // Switch 16k here, fix last at 0xC000
            }
        } else {
            match self.prg_rom_bank_mode() {
                Switch32 => (self.prg_bank & 0xFE) | 1,
                FixFirst => self.prg_bank, // Switch 16k here, first bank is fixed at 0x8000
                FixLast => (self.cart.num_prg_banks - 1) as u8, // Fix last bank
            }
        };
        u16::from(bank)
    }

    // Writes data into a shift register. At every 5th
    // write, the data is written out to the SxRom registers
    // and the shift register is cleared
    fn write_registers(&mut self, addr: u16, val: u8) {
        // Check reset
        if val & 0x80 != 0 {
            self.shift_register = 0x10;
            self.ctrl |= 0x0C;
            return;
        }

        // Check if its time to write
        let write = self.shift_register & 1 == 1;

        // Move shift register and write lowest bit of val
        self.shift_register >>= 1;
        self.shift_register |= (val & 1) << 4;

        if write {
            match addr {
                0x8000..=0x9FFF => self.ctrl = self.shift_register,
                0xA000..=0xBFFF => self.chr_bank_0 = self.shift_register,
                0xC000..=0xDFFF => self.chr_bank_1 = self.shift_register,
                0xE000..=0xFFFF => self.prg_bank = self.shift_register,
                _ => panic!("impossible write"),
            }
        }
    }
}

impl Board for Sxrom {
    fn scanline_irq(&self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        self.cart.mirroring
    }
    fn step(&mut self) {
        // NOOP
    }
}

impl Memory for Sxrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU 4 KB switchable CHR bank
            0x0000..=0x1FFF => {
                if self.cart.num_chr_banks == 0 {
                    self.cart.prg_ram[(addr & 0x1FFF) as usize]
                } else {
                    self.cart.chr_rom[(addr & 0x1FFF) as usize]
                }
            }
            // CPU 8 KB PRG RAM bank, (optional)
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            // CPU 2x16 KB PRG ROM bank, either switchable or fixed to the first bank
            0x8000..=0xFFFF => {
                let bank = self.get_prg_rom_bank(addr);
                let bank_size = PRG_BANK_SIZE;
                let addr = (bank as usize * bank_size) | (addr as usize & (bank_size - 1));
                self.cart.prg_rom[addr as usize]
            }
            _ => {
                eprintln!("unhandled Sxrom readb at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            // PPU 4 KB switchable CHR bank
            0x0000..=0x1FFF => {
                if self.cart.num_chr_banks == 0 {
                    self.cart.prg_ram[(addr & 0x1FFF) as usize] = val;
                } else {
                    self.cart.chr_rom[(addr & 0x1FFF) as usize] = val;
                }
            }
            // CPU 8 KB PRG RAM bank, (optional)
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize] = val,
            0x8000..=0xFFFF => {
                self.write_registers(addr, val);
            }
            _ => {
                eprintln!(
                    "unhandled Sxrom writeb at address: 0x{:04X} - val: 0x{:02X}",
                    addr, val
                );
            }
        }
        self.shift_register = 0x10; // Reset shift
    }
}

impl fmt::Debug for Sxrom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Sxrom {{ }}",)
    }
}

/// CNROM Board (Mapper 3)
///
/// https://wiki.nesdev.com/w/index.php/CNROM
/// https://wiki.nesdev.com/w/index.php/INES_Mapper_003

#[derive(Debug)]
pub struct Cnrom {
    cart: Cartridge,
    chr_bank: u16, // $0000-$1FFF 8K CHR-ROM
    prg_bank_1: u16,
    prg_bank_2: u16,
}

impl Cnrom {
    pub fn load(cart: Cartridge) -> Self {
        let prg_bank_2 = (cart.num_prg_banks - 1) as u16;
        Self {
            cart,
            chr_bank: 016,
            prg_bank_1: 016,
            prg_bank_2,
        }
    }
}

impl Memory for Cnrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            // $0000-$1FFF PPU
            0x0000..=0x1FFF => {
                let addr = self.chr_bank * 0x2000 + addr;
                if self.cart.num_chr_banks == 0 {
                    self.cart.prg_rom[addr as usize]
                } else {
                    self.cart.chr_rom[addr as usize]
                }
            }
            0x6000..=0x7FFF => self.cart.prg_ram[(addr - 0x6000) as usize],
            // $8000-$FFFF CPU
            0x8000..=0xBFFF => {
                let addr = self.prg_bank_1 * 0x4000 + (addr - 0x8000);
                self.cart.prg_rom[addr as usize]
            }
            0xC000..=0xFFFF => {
                let addr = self.prg_bank_2 * 0x4000 + (addr - 0xC000);
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
            // $0000-$1FFF PPU
            0x0000..=0x1FFF => {
                if self.cart.num_chr_banks == 0 {
                    let addr = self.chr_bank * 0x2000 + addr;
                    self.cart.prg_rom[addr as usize] = val;
                }
            }
            0x6000..=0x7FFF => self.cart.prg_ram[(addr - 0x6000) as usize] = val,
            // $8000-$FFFF CPU
            0x8000..=0xFFFF => self.chr_bank = u16::from(val & 3),
            _ => eprintln!("unhandled Cnrom readb at address: 0x{:04X}", addr),
        }
    }
}

impl Board for Cnrom {
    fn scanline_irq(&self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        self.cart.mirroring
    }
    fn step(&mut self) {
        // NOOP
    }
}
