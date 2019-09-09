//! ExROM/MMC5 (Mapper 5)
//!
//! [https://wiki.nesdev.com/w/index.php/ExROM]()
//! [https://wiki.nesdev.com/w/index.php/MMC5]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::mapper::Mirroring;
use crate::mapper::{Mapper, MapperRef};
use crate::memory::{Banks, Memory, Ram, Rom};
use crate::serialization::Savable;
use crate::util::Result;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
const PRG_ROM_BANK_SIZE: usize = 8 * 1024;
const CHR_ROM_BANK_SIZE: usize = 1 * 1024;
const PRG_RAM_SIZE: usize = 64 * 1024;
const EX_RAM_SIZE: usize = 1 * 1024;
const ROM_BANK: usize = 0xFFFF;

/// ExROM
#[derive(Debug)]
pub struct Exrom {
    regs: ExRegs,
    mirroring: Mirroring,
    battery_backed: bool,
    prg_rom: Rom,
    prg_rom_banks: [u32; 4],
}

#[derive(Debug)]
pub struct ExRegs {
    // 0: 0: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    1: CPU $8000-$FFFF: 32 KB switchable PRG ROM bank
    // 1: 0: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    1/0: CPU $8000-$BFFF: 16 KB switchable PRG ROM/RAM bank
    //      1: CPU $C000-$FFFF: 16 KB switchable PRG ROM bank
    // 2: 0: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    1/0: CPU $8000-$BFFF: 16 KB switchable PRG ROM/RAM bank
    //    2/1: CPU $C000-$DFFF: 8 KB switchable PRG ROM/RAM bank
    //      2: CPU $E000-$FFFF: 8 KB switchable PRG ROM bank
    // 3: 0: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    1/0: CPU $8000-$9FFF: 8 KB switchable PRG ROM/RAM bank
    //    2/1: CPU $A000-$BFFF: 8 KB switchable PRG ROM/RAM bank
    //    3/2: CPU $C000-$DFFF: 8 KB switchable PRG ROM/RAM bank
    //      3: CPU $E000-$FFFF: 8 KB switchable PRG ROM bank
    prg_mode: u8,
    // 0: 0: PPU $0000-$1FFF: 8 KB switchable CHR bank
    // 1: 0: PPU $0000-$0FFF: 4 KB switchable CHR bank
    //    1: PPU $1000-$1FFF: 4 KB switchable CHR bank
    // 2: 0: PPU $0000-$07FF: 2 KB switchable CHR bank
    //    1: PPU $0800-$0FFF: 2 KB switchable CHR bank
    //    2: PPU $1000-$17FF: 2 KB switchable CHR bank
    //    3: PPU $1800-$1FFF: 2 KB switchable CHR bank
    // 3: 0: PPU $0000-$03FF: 1 KB switchable CHR bank
    //    1: PPU $0400-$07FF: 1 KB switchable CHR bank
    //    2: PPU $0800-$0BFF: 1 KB switchable CHR bank
    //    3: PPU $0C00-$0FFF: 1 KB switchable CHR bank
    //    4: PPU $1000-$13FF: 1 KB switchable CHR bank
    //    5: PPU $1400-$17FF: 1 KB switchable CHR bank
    //    6: PPU $1800-$1BFF: 1 KB switchable CHR bank
    //    7: PPU $1C00-$1FFF: 1 KB switchable CHR bank
    chr_mode: u8,
    sprite8x16: bool,        // $2000 PPUCTRL: false = 8x8, true = 8x16
    rendering_enabled: bool, // $2001 PPUMASK: false = rendering disabled, true = enabled
    prg_ram_protect1: bool,  // $5102: Write $02 to enable PRG RAM writing
    prg_ram_protect2: bool,  // $5103: Write $01 to enable PRG RAM writing
    // $5104
    // 0 - Use as extra nametable (possibly for split mode)
    // 1 - Use as extended attribute data (can also be used as extended nametable)
    // 2 - Use as ordinary RAM
    // 3 - Use as ordinary RAM, write protected
    extended_ram_mode: u8,
    fill_tile: u8,             // $5106
    fill_attr: u8,             // $5107
    vertical_split_mode: u8,   // $5200
    vertical_split_scroll: u8, // $5201
    vertical_split_bank: u8,   // $5202
    scanline_num_irq: u8,      // $5203: Write $00 to disable IRQs
    irq_enabled: u8,           // $5204
    multiplicand: u8,          // $5205: write
    multiplier: u8,            // $5206: write
    mult_result: u16,          // $5205: read lo, $5206: read hi
    open_bus: u8,
}

// 8k  16k  32k
// 0   0    0
// 1
// 2   1
// 3
// 4   2
// 5
// 6   3    1 : 0x8000
// 7
// 8   4
// 9
// 10  5
// 11
// 12  6    2 : 0x10000
// 13
// 14  7
// 15
// 16  8
// 17
// 18  9    3 : 0x18000
// 19
// 20  10
// 21
// 22  11
// 23
// 24 -12   4 : 0x20000
// 25
// 26  13
// 27
// 28  14
// 29
// 30  15     : 0x3C000
// 31         : 0x3E000
impl Exrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let exrom = Self {
            regs: ExRegs {
                prg_mode: 3u8, // Default to mode 3
                chr_mode: 3u8, // Default to mode 3
                sprite8x16: false,
                rendering_enabled: true,
                prg_ram_protect1: false,
                prg_ram_protect2: false,
                extended_ram_mode: 0u8,
                fill_tile: 0xFFu8,
                fill_attr: 0xFFu8,
                vertical_split_mode: 0u8,
                vertical_split_scroll: 0u8,
                vertical_split_bank: 0u8,
                scanline_num_irq: 0u8,
                irq_enabled: 0u8,
                multiplicand: 0xFF,
                multiplier: 0xFF,
                mult_result: 0xFE01,
                open_bus: 0u8,
            },
            mirroring: cart.mirroring(),
            battery_backed: cart.battery_backed(),
            prg_rom: cart.prg_rom,
            prg_rom_banks: [24 * 8 * 1024, 0, 0, 30 * 8 * 1024],
        };
        Rc::new(RefCell::new(exrom))
    }

    // 7--- ---0
    // RAAA AaAA
    // |||| ||||
    // |||| |||+- PRG ROM/RAM A13
    // |||| ||+-- PRG ROM/RAM A14
    // |||| |+--- PRG ROM/RAM A15, also selecting between PRG RAM /CE 0 and 1
    // |||| +---- PRG ROM/RAM A16
    // |||+------ PRG ROM A17
    // ||+------- PRG ROM A18
    // |+-------- PRG ROM A19
    // +--------- RAM/ROM toggle (0: RAM; 1: ROM) (registers $5114-$5116 only)
    fn write_prg_bankswitching(&mut self, addr: u16, val: u8) {
        // TODO
    }

    fn write_chr_bankswitching(&mut self, addr: u16, val: u8) {
        // TODO
    }

    fn multiplier(&mut self, val: u8) {
        self.regs.mult_result = u16::from(self.regs.multiplicand) * u16::from(val);
    }

    fn read_expansion_ram(&self, addr: u16) -> u8 {
        // TODO
        0
    }

    fn write_expansion_ram(&mut self, addr: u16, val: u8) {
        // TODO
    }

    fn write_prg_ram(&mut self, addr: u16, val: u8) {
        // TODO
    }

    fn read_prg_mode(&self, addr: u16) -> u8 {
        // TODO
        0
    }

    fn write_prg_mode(&mut self, addr: u16, val: u8) {
        // TODO
    }

    fn set_mirroring(&mut self, val: u8) {
        self.mirroring = match val {
            0x50 => Mirroring::Horizontal,
            0x44 => Mirroring::Vertical,
            0x00 => Mirroring::SingleScreen0,
            0x55 => Mirroring::SingleScreen1,
            0xE4 => Mirroring::FourScreen,
            0xFF => panic!("Fill mode not supported yet"),
            _ => panic!("impossible mirroring mode"),
        }
    }
}

impl Mapper for Exrom {
    fn irq_pending(&mut self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn clock(&mut self, ppu: &Ppu) {
        // 5th bit of PPUCTRL
        self.regs.sprite8x16 = ppu.regs.ctrl.0 & 0x20 == 0x20;
        // 4th & 5th bits of PPUMASK
        self.regs.rendering_enabled = ppu.regs.mask.0 & 0x18 > 0;
    }
    fn battery_backed(&self) -> bool {
        false
    }
    fn save_sram(&self, _fh: &mut Write) -> Result<()> {
        Ok(())
    }
    fn load_sram(&mut self, _fh: &mut Read) -> Result<()> {
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
    fn reset(&mut self) {
        self.regs.prg_mode = 3;
        self.regs.chr_mode = 3;
    }
    fn power_cycle(&mut self) {
        self.reset();
    }
}

impl Memory for Exrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                // CHR ROM
                0
            }
            0x5204 => self.regs.irq_enabled,
            0x5205 => (self.regs.mult_result & 0xFF) as u8,
            0x5206 => ((self.regs.mult_result >> 8) & 0xFF) as u8,
            0x5208 => 0, // MMC5A only CL3 / SL3 Status
            0x5209 => 0, // MMC5A only 6-bit Hardware Timer with IRQ
            0x5C00..=0x5FFF => self.read_expansion_ram(addr),
            0x6000..=0x7FFF => 0,
            0x8000..=0xDFFF => self.read_prg_mode(addr),
            0xE000..=0xFFFF => {
                let bank = (addr >> 12) / 4;
                let offset = self.prg_rom_banks[bank as usize];
                panic!(
                    "off: ${:05X}, addr: ${:04X}, idx: ${:05X}, val: ${:04X}",
                    offset,
                    addr,
                    offset + (addr as u32 - 0xE000),
                    self.prg_rom[(offset + (addr as u32 % PRG_ROM_BANK_SIZE as u32)) as usize],
                );
                self.prg_rom[(offset + (addr as u32 % PRG_ROM_BANK_SIZE as u32)) as usize]
            }
            _ => {
                eprintln!("invalid Exrom read at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => (), // ROM is write-only
            0x5000..=0x5003 => (), // TODO Sound Pulse 1
            0x5004..=0x5007 => (), // TODO Sound Pulse 2
            0x5010..=0x5011 => (), // TODO Sound PCM
            0x5015 => (),          // TODO Sound General
            0x5100 => {
                println!("prg_mode {}", val & 0x03);
                self.regs.prg_mode = val & 0x03;
            }
            0x5101 => self.regs.chr_mode = val & 0x03,
            0x5102 => self.regs.prg_ram_protect1 = val & 0x03 != 0x02,
            0x5103 => self.regs.prg_ram_protect2 = val & 0x03 != 0x01,
            0x5104 => self.regs.extended_ram_mode = val & 0x03,
            0x5105 => self.set_mirroring(val),
            0x5106 => self.regs.fill_tile = val,
            0x5107 => self.regs.fill_attr = val & 0x03,
            0x5113..=0x5117 => self.write_prg_bankswitching(addr, val),
            0x5120..=0x512B => self.write_chr_bankswitching(addr, val),
            0x5130 => (), // TODO CHR high bits
            0x5200 => self.regs.vertical_split_mode = val,
            0x5201 => self.regs.vertical_split_scroll = val,
            0x5202 => self.regs.vertical_split_bank = val,
            0x5203 => self.regs.scanline_num_irq = val,
            0x5204 => self.regs.irq_enabled = val,
            0x5205 => self.regs.multiplicand = val,
            0x5206 => self.multiplier(val),
            0x5207 => (), // MMC5A only CL3 / SL3 Data Direction and Output Data Source
            0x5208 => (), // MMC5A only CL3 / SL3 Status
            0x5209 => (), // MMC5A only 6-bit Hardware Timer with IRQ
            0x5C00..=0x5FFF => self.write_expansion_ram(addr, val),
            0x6000..=0x7FFF => self.write_prg_ram(addr, val),
            0x8000..=0xDFFF => self.write_prg_mode(addr, val),
            0xE000..=0xFFFF => (), // ROM is write-only
            _ => eprintln!(
                "invalid Exrom write at address: 0x{:04X} - val: 0x{:02X}",
                addr, val
            ),
        }
    }
}

impl Savable for Exrom {
    fn save(&self, fh: &mut Write) -> Result<()> {
        // TODO
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        // TODO
        Ok(())
    }
}
