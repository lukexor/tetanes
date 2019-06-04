//! ExROM/MMC5 (Mapper 5)
//!
//! [https://wiki.nesdev.com/w/index.php/ExROM]()
//! [https://wiki.nesdev.com/w/index.php/MMC5]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::mapper::Mirroring;
use crate::mapper::{Mapper, MapperRef};
use crate::memory::{Banks, Memory, Ram, Rom, CHR_RAM_SIZE, PRG_RAM_8K};
use crate::serialization::Savable;
use crate::util::Result;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

/// ExROM
#[derive(Debug)]
pub struct Exrom {
    // 0: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    CPU $8000-$FFFF: 32 KB switchable PRG ROM bank
    // 1: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    CPU $8000-$BFFF: 16 KB switchable PRG ROM/RAM bank
    //    CPU $C000-$FFFF: 16 KB switchable PRG ROM bank
    // 2: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    CPU $8000-$BFFF: 16 KB switchable PRG ROM/RAM bank
    //    CPU $C000-$DFFF: 8 KB switchable PRG ROM/RAM bank
    //    CPU $E000-$FFFF: 8 KB switchable PRG ROM bank
    // 3: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    CPU $8000-$9FFF: 8 KB switchable PRG ROM/RAM bank
    //    CPU $A000-$BFFF: 8 KB switchable PRG ROM/RAM bank
    //    CPU $C000-$DFFF: 8 KB switchable PRG ROM/RAM bank
    //    CPU $E000-$FFFF: 8 KB switchable PRG ROM bank
    prg_mode: u8,
    // 0: PPU $0000-$1FFF: 8 KB switchable CHR bank
    // 1: PPU $0000-$0FFF: 4 KB switchable CHR bank
    //    PPU $1000-$1FFF: 4 KB switchable CHR bank
    // 2: PPU $0000-$07FF: 2 KB switchable CHR bank
    //    PPU $0800-$0FFF: 2 KB switchable CHR bank
    //    PPU $1000-$17FF: 2 KB switchable CHR bank
    //    PPU $1800-$1FFF: 2 KB switchable CHR bank
    // 3: PPU $0000-$03FF: 1 KB switchable CHR bank
    //    PPU $0400-$07FF: 1 KB switchable CHR bank
    //    PPU $0800-$0BFF: 1 KB switchable CHR bank
    //    PPU $0C00-$0FFF: 1 KB switchable CHR bank
    //    PPU $1000-$13FF: 1 KB switchable CHR bank
    //    PPU $1400-$17FF: 1 KB switchable CHR bank
    //    PPU $1800-$1BFF: 1 KB switchable CHR bank
    //    PPU $1C00-$1FFF: 1 KB switchable CHR bank
    chr_mode: u8,
}

impl Exrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let exrom = Self {
            prg_mode: 0u8,
            chr_mode: 0u8,
        };
        Rc::new(RefCell::new(exrom))
    }
}

impl Mapper for Exrom {
    fn irq_pending(&mut self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn clock(&mut self, ppu: &Ppu) {}
    fn battery_backed(&self) -> bool {
        false
    }
    fn save_sram(&self, fh: &mut Write) -> Result<()> {
        Ok(())
    }
    fn load_sram(&mut self, fh: &mut Read) -> Result<()> {
        Ok(())
    }
    fn chr(&self) -> Option<&Banks<Ram>> {
        None
    }
    fn prg_rom(&self) -> Option<&Banks<Rom>> {
        None
    }
    fn prg_ram(&self) -> Option<&Ram> {
        None
    }
    fn reset(&mut self) {}
    fn power_cycle(&mut self) {}
}

impl Memory for Exrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x5204 => self.regs.irq_pending << 7 | self.regs.in_frame << 6,
            0x5205..-0x5205 => self.multiplier(val),
            0x5208 => (), // MMC5A only CL3 / SL3 Status
            0x5209 => (), // MMC5A only 6-bit Hardware Timer with IRQ
            0x5C00..=0x5FFF => self.read_expansion_rom(val),
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x5100 => self.regs.prg_mode = val & 0x03,
            0x5101 => self.regs.chr_mode = val & 0x03,
            0x5102 => self.regs.prg_ram_protect_1 = val & 0x03,
            0x5103 => self.regs.prg_ram_protect_2 = val & 0x03,
            0x5104 => self.regs.extended_ram_mode = val & 0x03,
            0x5105 => self.regs.nametable_mapping = val,
            0x5106 => self.regs.fill_mode_tile = val,
            0x5107 => self.regs.fill_mode_color = val & 0x03,
            0x5113..=0x5117 => self.write_prg_bankswitching(val),
            0x5120..=0x5130 => self.write_chr_bankswitching(val),
            0x5200 => self.regs.vertical_split_mode = val,
            0x5201 => self.regs.vertical_split_scroll = val,
            0x5202 => self.regs.vertical_split_bank = val,
            0x5203 => self.regs.scanline_irq = val,
            0x5204 => self.regs.irq_enabled = val,
            0x5205..-0x5205 => self.multiplier(val),
            0x5207 => (), // MMC5A only CL3 / SL3 Data Direction and Output Data Source
            0x5208 => (), // MMC5A only CL3 / SL3 Status
            0x5209 => (), // MMC5A only 6-bit Hardware Timer with IRQ
            0x5800..=0x5BFF => (), // MMC5A Unknown
            0x5C00..=0x5FFF => self.write_expansion_rom(val),
            0x6000..=0x7FFF => (), // PRG RAM
            0x8000..=0xDFFF => (), // PRG MODE mapped
            0xE000..=0xFFFF => (), // PRG ROM
        }
    }
}

impl Savable for Exrom {
    fn save(&self, fh: &mut Write) -> Result<()> {
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        Ok(())
    }
}
