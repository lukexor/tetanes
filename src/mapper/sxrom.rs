//! SxROM/MMC1 (Mapper 1)
//!
//! [http://wiki.nesdev.com/w/index.php/SxROM]()
//! [http://wiki.nesdev.com/w/index.php/MMC1]()

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
const CHR_BANK_SIZE: usize = 4 * 1024;
const PRG_RAM_SIZE: usize = 32 * 1024; // 32KB is safely compatible sans NES 2.0 header
const CHR_RAM_SIZE: usize = 8 * 1024;

const SHIFT_REG_RESET: u8 = 0x80; // Reset shift register when bit 7 is set
const DEFAULT_SHIFT_REGISTER: u8 = 0x10; // 0b10000 the 1 is used to tell when register is full
const MIRRORING_MASK: u8 = 0x03; // 0b00011
const PRG_MODE_MASK: u8 = 0x0C; // 0b01100
                                // Mode 1 is 0 or 1 for switch32
const PRG_MODE_FIX_FIRST: u8 = 0x08; // Mode 2
const PRG_MODE_FIX_LAST: u8 = 0x0C; // Mode 3
const CHR_MODE_MASK: u8 = 0x10; // 0b10000
const PRG_RAM_DISABLED: u8 = 0x10; // 0b10000

/// SxROM
#[derive(Debug)]
pub struct Sxrom {
    regs: SxRegs,
    battery_backed: bool,
    prg_rom_bank_lo: usize,
    prg_rom_bank_hi: usize,
    chr_bank_lo: usize,
    chr_bank_hi: usize,
    prg_ram: Ram, // CPU $6000..=$7FFF 8K PRG RAM Bank (optional)
    // CPU $8000..=$BFFF 16KB PRG ROM Bank Switchable or Fixed to First Bank
    // CPU $C000..=$FFFF 16KB PRG ROM Bank Fixed to Last Bank or Switchable
    prg_rom_banks: Banks<Rom>,
    chr_banks: Banks<Ram>, // PPU $0000..=$1FFF 2 4KB CHR ROM/RAM Bank Switchable
}

#[derive(Debug)]
struct SxRegs {
    write_just_occurred: u8,
    shift_register: u8, // $8000-$FFFF - 5 bit shift register
    control: u8,        // $8000-$9FFF
    chr_bank_0: u8,     // $A000-$BFFF
    chr_bank_1: u8,     // $C000-$DFFF
    prg_bank: u8,       // $E000-$FFFF
    open_bus: u8,
}

impl Sxrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_ram_size = if cart.prg_ram_size() > 0 {
            cart.prg_ram_size()
        } else {
            PRG_RAM_SIZE
        };
        let prg_ram = Ram::init(prg_ram_size);
        let prg_rom_banks = Banks::init(&cart.prg_rom, PRG_ROM_BANK_SIZE);
        let chr_banks = if cart.chr_rom.len() == 0 {
            let chr_ram = Ram::init(CHR_RAM_SIZE);
            Banks::init(&chr_ram, CHR_BANK_SIZE)
        } else {
            Banks::init(&cart.chr_rom.to_ram(), CHR_BANK_SIZE)
        };
        let sxrom = Self {
            regs: SxRegs {
                write_just_occurred: 0u8,
                shift_register: DEFAULT_SHIFT_REGISTER,
                control: 0u8,
                chr_bank_0: 0u8,
                chr_bank_1: 0u8,
                prg_bank: PRG_MODE_FIX_LAST,
                open_bus: 0u8,
            },
            battery_backed: cart.battery_backed(),
            prg_rom_bank_lo: 0usize,
            prg_rom_bank_hi: prg_rom_banks.len() - 1,
            chr_bank_lo: 0usize,
            chr_bank_hi: 0usize,
            prg_ram,
            prg_rom_banks,
            chr_banks,
        };
        Rc::new(RefCell::new(sxrom))
    }

    /// Writes data into a shift register. At every 5th
    /// write, the data is written out to the SxRom registers
    /// and the shift register is cleared
    ///
    /// Load Register $8000-$FFFF
    /// 7654 3210
    /// Rxxx xxxD
    /// |       +- Data bit to be shifted into shift register, LSB first
    /// +--------- 1: Reset shift register and write control with (Control OR $0C),
    ///               locking PRG ROM at $C000-$FFFF to the last bank.
    ///
    /// Control $8000-$9FFF
    /// 43210
    /// CPPMM
    /// |||++- Mirroring (0: one-screen, lower bank; 1: one-screen, upper bank;
    /// |||               2: vertical; 3: horizontal)
    /// |++--- PRG ROM bank mode (0, 1: switch 32 KB at $8000, ignoring low bit of bank number;
    /// |                         2: fix first bank at $8000 and switch 16 KB bank at $C000;
    /// |                         3: fix last bank at $C000 and switch 16 KB bank at $8000)
    /// +----- CHR ROM bank mode (0: switch 8 KB at a time; 1: switch two separate 4 KB banks)
    ///
    /// CHR bank 0 $A000-$BFFF
    /// 42310
    /// CCCCC
    /// +++++- Select 4 KB or 8 KB CHR bank at PPU $0000 (low bit ignored in 8 KB mode)
    ///
    /// CHR bank 1 $C000-$DFFF
    /// 43210
    /// CCCCC
    /// +++++- Select 4 KB CHR bank at PPU $1000 (ignored in 8 KB mode)
    ///
    /// For Mapper001
    /// $A000 and $C000:
    /// 43210
    /// EDCBA
    /// |||||
    /// ||||+- CHR A12
    /// |||+-- CHR A13, if extant (CHR >= 16k)
    /// ||+--- CHR A14, if extant; and PRG RAM A14, if extant (PRG RAM = 32k)
    /// |+---- CHR A15, if extant; and PRG RAM A13, if extant (PRG RAM >= 16k)
    /// +----- CHR A16, if extant; and PRG ROM A18, if extant (PRG ROM = 512k)
    ///
    /// PRG bank $E000-$FFFF
    /// 43210
    /// RPPPP
    /// |++++- Select 16 KB PRG ROM bank (low bit ignored in 32 KB mode)
    /// +----- PRG RAM chip enable (0: enabled; 1: disabled; ignored on MMC1A)
    fn write_registers(&mut self, addr: u16, val: u8) {
        if self.regs.write_just_occurred > 0 {
            return;
        }
        self.regs.write_just_occurred = 6;
        if val & SHIFT_REG_RESET == SHIFT_REG_RESET {
            self.regs.shift_register = DEFAULT_SHIFT_REGISTER;
            self.regs.control |= PRG_MODE_FIX_LAST;
        } else {
            // Check if its time to write
            let write = self.regs.shift_register & 1 == 1;
            // Move shift register and write lowest bit of val
            self.regs.shift_register >>= 1;
            self.regs.shift_register |= (val & 1) << 4;
            if write {
                match addr {
                    0x8000..=0x9FFF => self.regs.control = self.regs.shift_register,
                    0xA000..=0xBFFF => self.regs.chr_bank_0 = self.regs.shift_register,
                    0xC000..=0xDFFF => self.regs.chr_bank_1 = self.regs.shift_register,
                    0xE000..=0xFFFF => self.regs.prg_bank = self.regs.shift_register,
                    _ => panic!("impossible write"),
                }
                self.regs.shift_register = DEFAULT_SHIFT_REGISTER;
                self.update_banks();
            }
        }
    }

    fn update_banks(&mut self) {
        let prg_len = self.prg_rom_banks.len();
        match self.regs.control & PRG_MODE_MASK {
            PRG_MODE_FIX_FIRST => {
                self.prg_rom_bank_lo = 0;
                self.prg_rom_bank_hi = (self.regs.prg_bank as usize) % prg_len;
            }
            PRG_MODE_FIX_LAST => {
                self.prg_rom_bank_lo = (self.regs.prg_bank as usize) % prg_len;
                self.prg_rom_bank_hi = prg_len - 1;
            }
            _ => {
                // Switch32
                self.prg_rom_bank_lo = ((self.regs.prg_bank & 0xFE) as usize) % prg_len;
                self.prg_rom_bank_hi = ((self.regs.prg_bank | 0x01) as usize) % prg_len;
            }
        }

        let chr_len = self.chr_banks.len();
        if self.regs.control & CHR_MODE_MASK == CHR_MODE_MASK {
            self.chr_bank_lo = (self.regs.chr_bank_0 as usize) % chr_len;
            self.chr_bank_hi = (self.regs.chr_bank_1 as usize) % chr_len;
        } else {
            self.chr_bank_lo = ((self.regs.chr_bank_0 & 0xFE) as usize) % chr_len;
            self.chr_bank_hi = ((self.regs.chr_bank_0 | 0x01) as usize) % chr_len;
        }
    }

    fn prg_ram_enabled(&self) -> bool {
        self.regs.prg_bank & PRG_RAM_DISABLED == 0
    }
}

impl Mapper for Sxrom {
    fn irq_pending(&mut self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        match self.regs.control & MIRRORING_MASK {
            0 => Mirroring::SingleScreen0,
            1 => Mirroring::SingleScreen1,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => panic!("impossible mirroring mode"),
        }
    }
    fn vram_change(&mut self, _ppu: &Ppu, _addr: u16) {}
    fn clock(&mut self, _ppu: &Ppu) {
        if self.regs.write_just_occurred > 0 {
            self.regs.write_just_occurred -= 1;
        }
    }
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
    fn reset(&mut self) {
        self.regs.shift_register = DEFAULT_SHIFT_REGISTER;
        self.regs.prg_bank = PRG_MODE_FIX_LAST;
        self.prg_rom_bank_hi = self.prg_rom_banks.len() - 1;
        self.update_banks();
    }
    fn power_cycle(&mut self) {
        self.regs.write_just_occurred = 0;
        if self.battery_backed {
            for bank in &mut *self.chr_banks {
                *bank = Ram::init(bank.len());
            }
            self.prg_ram = Ram::init(self.prg_ram.len());
        }
        self.reset();
    }
}

impl Memory for Sxrom {
    fn read(&mut self, addr: u16) -> u8 {
        let val = self.peek(addr);
        self.regs.open_bus = val;
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x0FFF => self.chr_banks[self.chr_bank_lo].peek(addr),
            0x1000..=0x1FFF => self.chr_banks[self.chr_bank_hi].peek(addr - 0x1000),
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled() {
                    self.prg_ram.peek(addr - 0x6000)
                } else {
                    self.regs.open_bus
                }
            }
            0x8000..=0xBFFF => self.prg_rom_banks[self.prg_rom_bank_lo].peek(addr - 0x8000),
            0xC000..=0xFFFF => self.prg_rom_banks[self.prg_rom_bank_hi].peek(addr - 0xC000),
            _ => {
                eprintln!("unhandled Sxrom read at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.regs.open_bus = val;
        match addr {
            0x0000..=0x0FFF => self.chr_banks[self.chr_bank_lo].write(addr, val),
            0x1000..=0x1FFF => self.chr_banks[self.chr_bank_hi].write(addr - 0x1000, val),
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled() {
                    self.prg_ram.write(addr - 0x6000, val)
                }
            }
            0x8000..=0xFFFF => self.write_registers(addr, val),
            _ => eprintln!(
                "invalid Sxrom write at address: 0x{:04X} - val: 0x{:02X}",
                addr, val
            ),
        }
    }
}

impl Savable for Sxrom {
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        self.regs.save(fh)?;
        self.prg_ram.save(fh)?;
        self.prg_rom_bank_lo.save(fh)?;
        self.prg_rom_bank_hi.save(fh)?;
        self.chr_bank_lo.save(fh)?;
        self.chr_bank_hi.save(fh)?;
        self.prg_ram.save(fh)?;
        self.prg_rom_banks.save(fh)?;
        self.chr_banks.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
        self.regs.load(fh)?;
        self.prg_ram.load(fh)?;
        self.prg_rom_bank_lo.load(fh)?;
        self.prg_rom_bank_hi.load(fh)?;
        self.chr_bank_lo.load(fh)?;
        self.chr_bank_hi.load(fh)?;
        self.prg_ram.load(fh)?;
        self.prg_rom_banks.load(fh)?;
        self.chr_banks.load(fh)
    }
}

impl Savable for SxRegs {
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        self.write_just_occurred.save(fh)?;
        self.shift_register.save(fh)?;
        self.control.save(fh)?;
        self.chr_bank_0.save(fh)?;
        self.chr_bank_1.save(fh)?;
        self.prg_bank.save(fh)?;
        self.open_bus.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
        self.write_just_occurred.load(fh)?;
        self.shift_register.load(fh)?;
        self.control.load(fh)?;
        self.chr_bank_0.load(fh)?;
        self.chr_bank_1.load(fh)?;
        self.prg_bank.load(fh)?;
        self.open_bus.load(fh)
    }
}
