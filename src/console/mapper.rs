use super::memory::{Addr, Byte, Memory, Ram};
use crate::console::cartridge::{Board, Cartridge, ScanlineIrqResult, PRG_BANK_SIZE};
use std::fmt;

/// Nrom Board (mapper 0)
///
/// http://wiki.nesdev.com/w/index.php/NROM
pub struct Nrom {
    cart: Cartridge,
}

impl Nrom {
    pub fn load(cart: Cartridge) -> Self {
        Self { cart }
    }
}

impl Memory for Nrom {
    fn readb(&self, addr: Addr) -> Byte {
        match addr {
            // PPU 8K Fixed CHR bank
            0x0000..=0x1FFF => self.cart.chr_rom.readb(addr & 0x1FFF),
            0x6000..=0x7FFF => 0, // TODO PRG RAM - Family Basic only
            0x8000..=0xFFFF => {
                // CPU 32K Fixed PRG ROM bank for NROM-256
                if self.cart.prg_rom.len() > 0x4000 {
                    self.cart.prg_rom.readb(addr & 0x7FFF)
                // CPU 16K Fixed PRG ROM bank for NROM-128
                } else {
                    self.cart.prg_rom.readb(addr & 0x3FFF)
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
            _ => eprintln!(
                "invalid Nrom writeb at address: 0x{:04X} - val: 0x{:02X}",
                addr, val
            ),
        }
    }
}

impl Board for Nrom {
    fn scanline_irq(&self) -> ScanlineIrqResult {
        ScanlineIrqResult::Continue
    }
}

impl fmt::Debug for Nrom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Nrom {{ PRG-ROM: {}KB, CHR-RAM: {}KB }}",
            self.cart.prg_rom.len() / 0x0400,
            self.cart.chr_rom.len() / 0x0400,
        )
    }
}

/// SxRom

pub struct Sxrom {
    cart: Cartridge,
    // Registers
    ctrl: Byte,         // $8000-$9FFF
    chr_bank_0: Byte,   // $A000-$BFFF
    chr_bank_1: Byte,   // $C000-$DFFF
    prg_bank: Byte,     // $E000-$FFFF
    shift_register: u8, // Write every 5th write
    prg_ram: Ram,
    chr_ram: Ram,
}

enum SxMirroring {
    OneScreenLower,
    OneScreenUpper,
    Vertical,
    Horizontal,
}

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
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
            shift_register: 0x10,
            prg_ram: Ram::with_capacity(0x2000), // 8K
            chr_ram: Ram::with_capacity(0x2000), // 8K
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

    fn get_prg_rom_bank(&self, addr: Addr) -> u16 {
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
    fn write_registers(&mut self, addr: Addr, val: Byte) {
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
    fn scanline_irq(&self) -> ScanlineIrqResult {
        ScanlineIrqResult::Continue
    }
}

impl Memory for Sxrom {
    fn readb(&self, addr: u16) -> u8 {
        match addr {
            // PPU 4 KB switchable CHR bank
            0x0000..=0x1FFF => self.cart.chr_rom.readb(addr & 0x1FFF),
            // CPU 8 KB PRG RAM bank, (optional)
            0x6000..=0x7FFF => self.prg_ram.readb(addr - 0x6000),
            // CPU 2x16 KB PRG ROM bank, either switchable or fixed to the first bank
            0x8000..=0xFFFF => {
                let bank = self.get_prg_rom_bank(addr);
                let bank_size = PRG_BANK_SIZE as Addr;
                let addr = (bank * bank_size) | (addr & (bank_size - 1));
                self.cart.prg_rom.readb(addr)
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
            0x0000..=0x1FFF => self.cart.chr_rom.writeb(addr & 0x1FFF, val),
            // CPU 8 KB PRG RAM bank, (optional)
            0x6000..=0x7FFF => self.prg_ram.writeb(addr - 0x6000, val),
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
