//! ExROM/MMC5 (Mapper 5)
//!
//! [https://wiki.nesdev.com/w/index.php/ExROM]()
//! [https://wiki.nesdev.com/w/index.php/MMC5]()

use crate::cartridge::Cartridge;
use crate::console::debugger::Debugger;
use crate::console::ppu::{Ppu, PRERENDER_SCANLINE};
use crate::mapper::Mirroring;
use crate::mapper::{Mapper, MapperRef};
use crate::memory::{Banks, Memory, Ram, Rom};
use crate::serialization::Savable;
use crate::util::Result;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
const PRG_RAM_SIZE: usize = 64 * 1024;
const EX_RAM_SIZE: usize = 1 * 1024;

/// ExROM
#[derive(Debug)]
pub struct Exrom {
    regs: ExRegs,
    irq_pending: bool,
    mirroring: Mirroring,
    battery_backed: bool,
    prg_banks: [usize; 5],
    chr_banks_a: [usize; 8],
    chr_banks_b: [usize; 4],
    prg_ram: Ram,
    prg_rom: Rom,
    chr: Ram,
}

#[derive(Debug, PartialEq, Eq)]
enum ChrBank {
    A,
    B,
}

#[derive(Debug)]
pub struct ExRegs {
    // 0: 0: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    1: CPU $8000-$FFFF: 32 KB switchable PRG ROM bank
    // 1: 0: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    1: CPU $8000-$BFFF: 16 KB switchable PRG ROM/RAM bank
    //    2: CPU $C000-$FFFF: 16 KB switchable PRG ROM bank
    // 2: 0: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    1: CPU $8000-$BFFF: 16 KB switchable PRG ROM/RAM bank
    //    2: CPU $C000-$DFFF: 8 KB switchable PRG ROM/RAM bank
    //    3: CPU $E000-$FFFF: 8 KB switchable PRG ROM bank
    // 3: 0: CPU $6000-$7FFF: 8 KB switchable PRG RAM bank
    //    1: CPU $8000-$9FFF: 8 KB switchable PRG ROM/RAM bank
    //    2: CPU $A000-$BFFF: 8 KB switchable PRG ROM/RAM bank
    //    3: CPU $C000-$DFFF: 8 KB switchable PRG ROM/RAM bank
    //    4: CPU $E000-$FFFF: 8 KB switchable PRG ROM bank
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
    chr_hi_bit: usize,
    last_chr_write: ChrBank,
    sprite8x16: bool, // $2000 PPUCTRL: false = 8x8, true = 8x16
    sp_fetch_count: u16,
    prev_vram_addr: [u16; 2],
    rendering_enabled: bool, // $2001 PPUMASK: false = rendering disabled, true = enabled
    prg_ram_protect1: u8,    // $5102: Write $02 to enable PRG RAM writing
    prg_ram_protect2: u8,    // $5103: Write $01 to enable PRG RAM writing
    // $5104
    // 0 - Use as extra nametable (possibly for split mode)
    // 1 - Use as extended attribute data (can also be used as extended nametable)
    // 2 - Use as ordinary RAM
    // 3 - Use as ordinary RAM, write protected
    extended_ram_mode: u8,
    nametable_mapping: u8,     // $5105
    fill_tile: u8,             // $5106
    fill_attr: u8,             // $5107
    vertical_split_mode: u8,   // $5200
    vertical_split_scroll: u8, // $5201
    vertical_split_bank: u8,   // $5202
    scanline_num_irq: u8,      // $5203: Write $00 to disable IRQs
    irq_enabled: bool,         // $5204
    irq_counter: u8,
    in_frame: bool,
    multiplicand: u8, // $5205: write
    multiplier: u8,   // $5206: write
    mult_result: u16, // $5205: read lo, $5206: read hi
    open_bus: u8,
}

impl Exrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_ram = Ram::init(PRG_RAM_SIZE);
        let prg_len = cart.prg_rom.len();
        let num_rom_banks = cart.prg_rom.len() / (8 * 1024); // Default PRG ROM Bank size
        let mut exrom = Self {
            regs: ExRegs {
                prg_mode: 0xFF,
                chr_mode: 0xFF,
                chr_hi_bit: 0usize,
                last_chr_write: ChrBank::A,
                sprite8x16: false,
                sp_fetch_count: 0u16,
                prev_vram_addr: [0u16; 2],
                rendering_enabled: true,
                prg_ram_protect1: 0xFF,
                prg_ram_protect2: 0xFF,
                extended_ram_mode: 0xFF,
                nametable_mapping: 0xFF,
                fill_tile: 0xFF,
                fill_attr: 0xFF,
                vertical_split_mode: 0xFF,
                vertical_split_scroll: 0xFF,
                vertical_split_bank: 0xFF,
                scanline_num_irq: 0xFF,
                irq_enabled: false,
                irq_counter: 0u8,
                in_frame: false,
                multiplicand: 0xFF,
                multiplier: 0xFF,
                mult_result: 0xFE01,
                open_bus: 0u8,
            },
            irq_pending: false,
            mirroring: cart.mirroring(),
            battery_backed: cart.battery_backed(),
            prg_banks: [0; 5],
            chr_banks_a: [0; 8],
            chr_banks_b: [0; 4],
            prg_ram,
            prg_rom: cart.prg_rom,
            chr: cart.chr_rom.to_ram(),
        };
        exrom.prg_banks[3] = 0x80 | num_rom_banks - 2;
        exrom.prg_banks[4] = 0x80 | num_rom_banks - 1;
        for bank in 0..128 {
            let idx = bank * 1024;
            eprintln!("bank: {}", bank);
            Debugger::hexdump(&exrom.chr[idx..idx + 100]);
        }
        // eprintln!("Banks:");
        // for bank in exrom.prg_banks.iter() {
        //     let bank_size = match exrom.regs.prg_mode {
        //         0 => 32 * 1024,
        //         1 => 16 * 1024,
        //         2 => match bank {
        //             1 => 16 * 1024,
        //             _ => 8 * 1024,
        //         },
        //         3 | 0xFF => 8 * 1024,
        //         _ => panic!("invalid prg_mode"),
        //     };
        //     let idx = (bank & 0x7F) * bank_size;
        //     eprintln!("bank: {}", bank & 0x7F);
        //     if bank & 0x80 == 0x80 {
        //         Debugger::hexdump(&exrom.prg_rom[idx..idx + 100]);
        //     } else {
        //         Debugger::hexdump(&exrom.prg_ram[idx..idx + 100]);
        //     }
        // }
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
        let rom_mask = (val & 0x80) as usize;
        let bank = (val & 0x7F) as usize;
        match addr {
            0x5113 => self.prg_banks[0] = bank,
            0x5114 if self.regs.prg_mode == 0x03 => self.prg_banks[1] = bank,
            0x5115 => {
                match self.regs.prg_mode {
                    1 | 2 => self.prg_banks[1] = bank >> 1 | rom_mask,
                    3 => self.prg_banks[2] = bank,
                    _ => (), // Do nothing
                }
            }
            0x5116 => self.prg_banks[self.regs.prg_mode as usize] = bank,
            0x5117 => {
                let shift = 2usize.saturating_sub(self.regs.prg_mode as usize);
                self.prg_banks[self.regs.prg_mode as usize + 1] = (bank >> shift) | rom_mask;
            }
            _ => (), // Do nothing
        }
    }

    fn multiplier(&mut self, val: u8) {
        self.regs.mult_result = u16::from(self.regs.multiplicand) * u16::from(val);
    }

    fn read_expansion_ram(&self, addr: u16) -> u8 {
        // TODO
        eprintln!("Tried to read expansion ram");
        0
    }

    fn write_expansion_ram(&mut self, addr: u16, val: u8) {
        // TODO
        eprintln!("Tried to write expansion ram");
    }

    fn set_mirroring(&mut self, val: u8) {
        self.regs.nametable_mapping = val;
        self.mirroring = match val {
            0x50 => Mirroring::Horizontal,
            0x44 => Mirroring::Vertical,
            0x00 => Mirroring::SingleScreen0,
            0x55 => Mirroring::SingleScreen1,
            0xE4 => Mirroring::FourScreen,
            // _ => Mirroring::Horizontal,
            0xFF => {
                eprintln!("Fill mode not supported yet");
                Mirroring::Horizontal
            }
            _ => panic!("impossible mirroring mode"),
        }
    }
}

impl Mapper for Exrom {
    fn irq_pending(&mut self) -> bool {
        self.irq_pending
    }
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn vram_change(&mut self, ppu: &Ppu, addr: u16) {
        let vram_addr = addr;
        self.regs.sp_fetch_count = self.regs.sp_fetch_count.wrapping_add(1);

        if vram_addr == self.regs.prev_vram_addr[0] && vram_addr == self.regs.prev_vram_addr[1] {
            // not in-frame
            if !self.regs.in_frame {
                self.regs.in_frame = true;
                self.irq_pending = false;
                self.regs.irq_counter = 0;
            } else {
                self.regs.in_frame = true;
                self.regs.irq_counter = self.regs.irq_counter.wrapping_add(1);
                if self.regs.scanline_num_irq > 0
                    && self.regs.irq_counter == self.regs.scanline_num_irq
                    && self.regs.irq_enabled
                {
                    self.irq_pending = false;
                }
            }
            self.regs.sp_fetch_count = 0;
        }

        self.regs.prev_vram_addr[1] = self.regs.prev_vram_addr[0];
        self.regs.prev_vram_addr[0] = vram_addr;
    }
    fn clock(&mut self, ppu: &Ppu) {
        // 5th bit of PPUCTRL
        self.regs.sprite8x16 = ppu.regs.ctrl.0 & 0x20 == 0x20;
        // 4th & 5th bits of PPUMASK
        self.regs.rendering_enabled = ppu.regs.mask.0 & 0x18 > 0;
        if ppu.scanline >= PRERENDER_SCANLINE - 1 && ppu.scanline < PRERENDER_SCANLINE {
            self.regs.in_frame = false;
        }
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
        self.regs.prg_mode = 0xFF;
        self.regs.chr_mode = 0xFF;
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
                let mut bank_size = 1;
                let mut bank_idx_a = (addr >> 10) as usize;
                let mut bank_idx_b = ((addr >> 10) & 3) as usize;
                match self.regs.chr_mode {
                    0 => {
                        bank_size = 8;
                        bank_idx_a = 7;
                        bank_idx_b = 3;
                    }
                    1 => {
                        bank_size = 4;
                        bank_idx_a = if addr < 0x1000 { 3 } else { 7 };
                        bank_idx_b = 3;
                    }
                    2 => {
                        bank_size = 2;
                        bank_idx_a = match addr {
                            0x0000..=0x07FF => 1,
                            0x0800..=0x0FFF => 3,
                            0x1000..=0x17FF => 5,
                            0x1800..=0x1FFF => 7,
                            _ => panic!("invalid addr"),
                        };
                        bank_idx_b = match addr {
                            0x0000..=0x07FF => 1,
                            0x0800..=0x0FFF => 3,
                            0x1000..=0x17FF => 1,
                            0x1800..=0x1FFF => 3,
                            _ => panic!("invalid addr"),
                        };
                    }
                    _ => (), // Use Default
                }
                bank_size *= 1024;
                let bank = if self.regs.sprite8x16 {
                    if self.regs.sp_fetch_count > 128 && self.regs.sp_fetch_count < 160 {
                        self.chr_banks_a[bank_idx_a]
                    } else {
                        self.chr_banks_b[bank_idx_b]
                    }
                } else if self.regs.last_chr_write == ChrBank::A {
                    self.chr_banks_a[bank_idx_a]
                } else {
                    self.chr_banks_b[bank_idx_b]
                };
                let offset = addr as usize % bank_size;
                self.chr[bank * bank_size + offset]
            }
            0x5000..=0x5003 => 0, // TODO Sound Pulse 1
            0x5004..=0x5007 => 0, // TODO Sound Pulse 2
            0x5010..=0x5011 => 0, // TODO Sound PCM
            0x5015 => 0,          // TODO Sound General
            0x5100 => self.regs.prg_mode,
            0x5101 => self.regs.chr_mode,
            0x5102 => self.regs.prg_ram_protect1,
            0x5103 => self.regs.prg_ram_protect2,
            0x5104 => self.regs.extended_ram_mode,
            0x5105 => self.regs.nametable_mapping,
            0x5106 => self.regs.fill_tile,
            0x5107 => self.regs.fill_attr,
            0x5113..=0x5117 => 0,
            0x5120..=0x512B => 0,
            0x5130 => 0,
            0x5200 => self.regs.vertical_split_mode,
            0x5201 => self.regs.vertical_split_scroll,
            0x5202 => self.regs.vertical_split_bank,
            0x5203 => self.regs.scanline_num_irq,
            0x5204 => (self.irq_pending as u8) << 7 | (self.regs.in_frame as u8) << 6,
            0x5205 => (self.regs.mult_result & 0xFF) as u8,
            0x5206 => ((self.regs.mult_result >> 8) & 0xFF) as u8,
            0x5C00..=0x5FFF => self.read_expansion_ram(addr),
            0x6000..=0x7FFF => {
                let bank = self.prg_banks[(addr - 0x6000) as usize / PRG_RAM_BANK_SIZE];
                let offset = addr as usize % PRG_RAM_BANK_SIZE;
                self.prg_ram[bank * PRG_RAM_BANK_SIZE + offset]
            }
            0x8000..=0xFFFF => {
                let bank_size = match self.regs.prg_mode {
                    0 => 32 * 1024,
                    1 => 16 * 1024,
                    2 => match addr {
                        0x8000..=0xBFFF => 16 * 1024,
                        _ => 8 * 1024,
                    },
                    3 | 0xFF => 8 * 1024,
                    _ => panic!("invalid prg_mode"),
                };
                let bank = self.prg_banks[1 + (addr - 0x8000) as usize / bank_size];
                let offset = addr as usize % bank_size;
                // If bank is ROM
                if bank & 0x80 == 0x80 {
                    self.prg_rom[(bank & 0x7F) * bank_size + offset]
                } else {
                    self.prg_ram[(bank & 0x7F) * bank_size + offset]
                }
            }
            0x5207 => 0, // MMC5A only CL3 / SL3 Data Direction and Output Data Source
            0x5208 => 0, // MMC5A only CL3 / SL3 Status
            0x5209 => 0, // MMC5A only 6-bit Hardware Timer with IRQ
            _ => {
                eprintln!("invalid Exrom read at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => eprintln!("ROM is write-only"), // ROM is write-only
            0x5000..=0x5003 => eprintln!("Writing pulse 1"),   // TODO Sound Pulse 1
            0x5004..=0x5007 => eprintln!("Writing pulse 2"),   // TODO Sound Pulse 2
            0x5010..=0x5011 => eprintln!("Writing pcm"),       // TODO Sound PCM
            0x5015 => eprintln!("Writing sound general"),      // TODO Sound General
            0x5100 => self.regs.prg_mode = val & 0x03,
            0x5101 => self.regs.chr_mode = val & 0x03,
            0x5102 => self.regs.prg_ram_protect1 = val & 0x03,
            0x5103 => self.regs.prg_ram_protect2 = val & 0x03,
            0x5104 => self.regs.extended_ram_mode = val & 0x03,
            0x5105 => self.set_mirroring(val),
            0x5106 => self.regs.fill_tile = val,
            0x5107 => self.regs.fill_attr = val & 0x03,
            0x5113..=0x5117 => self.write_prg_bankswitching(addr, val),
            0x5120..=0x5127 => {
                self.regs.last_chr_write = ChrBank::A;
                self.chr_banks_a[(addr & 0x07) as usize] = val as usize | self.regs.chr_hi_bit;
                // if val > 1 && val != 127 {
                //     eprintln!("bank val: {}", val as usize | self.regs.chr_hi_bit);
                //     for (i, bank) in self.chr_banks_a.iter().enumerate() {
                //         if *bank > 0 && *bank != 127 {
                //             let idx = bank * 1024;
                //             eprintln!("bank: {}, idx: {}, size: {}", bank, idx, 1024);
                //             Debugger::hexdump(&self.chr[idx..idx + 100]);
                //         }
                //     }
                //     eprint!("> ");
                //     let mut input = String::new();
                //     std::io::stdin().read_line(&mut input);
                // }
            }
            0x5128..=0x512B => {
                self.regs.last_chr_write = ChrBank::B;
                self.chr_banks_b[(addr & 0x03) as usize] = val as usize | self.regs.chr_hi_bit;
            }
            0x5130 => self.regs.chr_hi_bit = (val as usize & 0x3) << 8,
            0x5200 => self.regs.vertical_split_mode = val,
            0x5201 => self.regs.vertical_split_scroll = val,
            0x5202 => self.regs.vertical_split_bank = val,
            0x5203 => self.regs.scanline_num_irq = val,
            0x5204 => self.regs.irq_enabled = val & 0x80 == 0x80,
            0x5205 => self.regs.multiplicand = val,
            0x5206 => self.multiplier(val),
            0x5C00..=0x5FFF => self.write_expansion_ram(addr, val),
            0x6000..=0x7FFF => {
                let bank = self.prg_banks[(addr - 0x6000) as usize / PRG_RAM_BANK_SIZE];
                let offset = addr as usize % PRG_RAM_BANK_SIZE;
                self.prg_ram[bank * PRG_RAM_BANK_SIZE + offset] = val;
            }
            0x8000..=0xDFFF => {
                let bank_size = match self.regs.prg_mode {
                    0 => 32 * 1024,
                    1 => 16 * 1024,
                    2 => match addr {
                        0x8000..=0xBFFF => 16 * 1024,
                        _ => 8 * 1024,
                    },
                    3 | 0xFF => 8 * 1024,
                    _ => panic!("invalid prg_mode"),
                };
                let bank = self.prg_banks[1 + (addr - 0x8000) as usize / bank_size];
                let offset = addr as usize % bank_size;
                if bank & 0x80 != 0x80
                    && self.regs.prg_ram_protect1 & 0x03 == 0x10
                    && self.regs.prg_ram_protect2 & 0x03 == 0x01
                {
                    self.prg_ram[(bank & 0x7F) * bank_size + offset];
                }
            }
            // 0x5207 => (), // MMC5A only CL3 / SL3 Data Direction and Output Data Source
            // 0x5208 => (), // MMC5A only CL3 / SL3 Status
            // 0x5209 => (), // MMC5A only 6-bit Hardware Timer with IRQ
            // 0xE000..=0xFFFF => (), // ROM is write-only
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
