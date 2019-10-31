//! ExROM/MMC5 (Mapper 5)
//!
//! [https://wiki.nesdev.com/w/index.php/ExROM]()
//! [https://wiki.nesdev.com/w/index.php/MMC5]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    debug,
    logging::{LogLevel, Loggable},
    mapper::{Mapper, MapperRef, Mirroring},
    memory::{MemRead, MemWrite, Memory},
    serialization::Savable,
    NesResult,
};
use std::{
    cell::RefCell,
    fmt,
    io::{Read, Write},
    rc::Rc,
};

const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
const PRG_RAM_SIZE: usize = 64 * 1024;
const EXRAM_SIZE: usize = 1024;

/// ExROM
pub struct Exrom {
    regs: ExRegs,
    mirroring: Mirroring,
    irq_pending: bool,
    last_chr_write: ChrBank,
    spr_fetch_count: u32,
    ppu_prev_addr: u16,
    ppu_prev_match: u8,
    ppu_reading: bool,
    ppu_idle: u8,
    ppu_in_vblank: bool,
    ppu_cycle: u16,
    ppu_rendering: bool,
    prg_banks: [usize; 5],
    chr_banks_spr: [usize; 8],
    chr_banks_bg: [usize; 4],
    cart: Cartridge,
    prg_ram: Memory,
    exram: Memory,
    log_level: LogLevel,
    open_bus: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ChrBank {
    Spr,
    Bg,
}

#[derive(Debug)]
pub struct ExRegs {
    sprite8x16: bool,          // $2000 PPUCTRL: false = 8x8, true = 8x16
    prg_mode: u8,              // $5100
    chr_mode: u8,              // $5101
    chr_hi_bit: u8,            // $5130
    prg_ram_protect_a: bool,   // $5102
    prg_ram_protect_b: bool,   // $5103
    exram_mode: u8,            // $5104
    nametable_mirroring: u8,   // $5105
    fill_tile: u8,             // $5106
    fill_attr: u8,             // $5107
    vertical_split_mode: u8,   // $5200
    vertical_split_scroll: u8, // $5201
    vertical_split_bank: u8,   // $5202
    scanline_num_irq: u16,     // $5203: Write $00 to disable IRQs
    irq_enabled: bool,         // $5204
    irq_counter: u16,
    in_frame: bool,
    multiplicand: u8, // $5205: write
    multiplier: u8,   // $5206: write
    mult_result: u16, // $5205: read lo, $5206: read hi
}

impl ExRegs {
    fn new(mirroring: Mirroring) -> Self {
        Self {
            sprite8x16: false,
            prg_mode: 0x03,
            chr_mode: 0x03,
            chr_hi_bit: 0u8,
            prg_ram_protect_a: false,
            prg_ram_protect_b: false,
            exram_mode: 0xFF,
            nametable_mirroring: match mirroring {
                Mirroring::Horizontal => 0x50,
                Mirroring::Vertical => 0x44,
                Mirroring::SingleScreenA => 0x00,
                Mirroring::SingleScreenB => 0x55,
                Mirroring::FourScreen => 0xFF,
            },
            fill_tile: 0xFF,
            fill_attr: 0xFF,
            vertical_split_mode: 0xFF,
            vertical_split_scroll: 0xFF,
            vertical_split_bank: 0xFF,
            scanline_num_irq: 0xFF,
            irq_enabled: false,
            irq_counter: 0u16,
            in_frame: false,
            multiplicand: 0xFF,
            multiplier: 0xFF,
            mult_result: 0xFE01,
        }
    }
}

impl Exrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_ram = Memory::ram(PRG_RAM_SIZE);
        let exram = Memory::ram(EXRAM_SIZE);
        let mirroring = cart.mirroring();
        let mut exrom = Self {
            regs: ExRegs::new(mirroring),
            mirroring,
            irq_pending: false,
            last_chr_write: ChrBank::Spr,
            spr_fetch_count: 0,
            ppu_prev_addr: 0xFFFF,
            ppu_prev_match: 0,
            ppu_reading: false,
            ppu_idle: 0,
            ppu_in_vblank: false,
            ppu_cycle: 0,
            ppu_rendering: false,
            prg_banks: [0; 5],
            chr_banks_spr: [0; 8],
            chr_banks_bg: [0; 4],
            cart,
            prg_ram,
            exram,
            log_level: LogLevel::default(),
            open_bus: 0x00,
        };
        exrom.prg_banks[4] = 0xFF; // $5117 defaults to last bank
        Rc::new(RefCell::new(exrom))
    }

    // $5113: [.... .CPP]
    //      8k PRG-RAM @ $6000
    //      C = Chip select
    // $5114-5117: [RPPP PPPP]
    //      R = ROM select (0=select RAM, 1=select ROM)  **unused in $5117**
    //      P = PRG page
    //
    //              $6000   $8000   $A000   $C000   $E000
    //            +-------+-------------------------------+
    // P=%00:     | $5113 |           <<$5117>>           |
    //            +-------+-------------------------------+
    // P=%01:     | $5113 |    <$5115>    |    <$5117>    |
    //            +-------+---------------+-------+-------+
    // P=%10:     | $5113 |    <$5115>    | $5116 | $5117 |
    //            +-------+---------------+-------+-------+
    // P=%11:     | $5113 | $5114 | $5115 | $5116 | $5117 |
    //            +-------+-------+-------+-------+-------+
    fn get_prg_addr(&self, addr: u16) -> (usize, bool) {
        let (bank_size, bank_idx) = match (addr, self.regs.prg_mode) {
            (0x6000..=0x7FFF, _) => (PRG_RAM_BANK_SIZE, 0),
            (_, 0) => (32 * 1024, 4),
            (_, 1) | (0x8000..=0xBFFF, 2) => (16 * 1024, 2 + (((addr - 0x8000) >> 14) << 1)),
            _ => (8 * 1024, 1 + ((addr - 0x8000) >> 13)),
        };
        let offset = addr as usize % bank_size;
        let bank = self.prg_banks[bank_idx as usize];
        let rom_select = bank & 0x80 > 0;
        let bank = match (self.regs.prg_mode, bank_idx) {
            (0, 4) => (bank & 0x7F) >> 2,
            (1, 2) | (1, 4) | (2, 2) => (bank & 0x7F) >> 1,
            _ => bank & 0x7F,
        };
        (bank * bank_size + offset, rom_select)
    }

    // 'A' Set (sprites):
    //               $0000   $0400   $0800   $0C00   $1000   $1400   $1800   $1C00
    //             +---------------------------------------------------------------+
    //   C=%00:    |                             $5127                             |
    //             +---------------------------------------------------------------+
    //   C=%01:    |             $5123             |             $5127             |
    //             +-------------------------------+-------------------------------+
    //   C=%10:    |     $5121     |     $5123     |     $5125     |     $5127     |
    //             +---------------+---------------+---------------+---------------+
    //   C=%11:    | $5120 | $5121 | $5122 | $5123 | $5124 | $5125 | $5126 | $5127 |
    //             +-------+-------+-------+-------+-------+-------+-------+-------+
    //
    // 'B' Set (BG):
    //               $0000   $0400   $0800   $0C00   $1000   $1400   $1800   $1C00
    //             +-------------------------------+-------------------------------+
    //   C=%00:    |                             $512B                             |
    //             +-------------------------------+-------------------------------+
    //   C=%01:    |             $512B             |             $512B             |
    //             +-------------------------------+-------------------------------+
    //   C=%10:    |     $5129     |     $512B     |     $5129     |     $512B     |
    //             +---------------+---------------+---------------+---------------+
    //   C=%11:    | $5128 | $5129 | $512A | $512B | $5128 | $5129 | $512A | $512B |
    //             +-------+-------+-------+-------+-------+-------+-------+-------+
    fn get_chr_addr(&self, addr: u16) -> usize {
        // 8K, 4K, 2K, or 1K
        let bank_size = (8 * 1024) / (1 << self.regs.chr_mode as usize);
        let offset = addr as usize % bank_size;
        let bank_idx = match self.regs.chr_mode {
            0 => 7,
            1 => 3 + ((addr >> 12) << 2),
            2 => 1 + ((addr >> 11) << 1),
            3 => addr >> 10,
            _ => panic!("invalid chr_mode"),
        } as usize;
        let bank = if self.regs.sprite8x16 {
            // Means we've gotten our 32 BG tiles fetched (32 * 4)
            if self.spr_fetch_count >= 127 && self.spr_fetch_count <= 158 {
                self.chr_banks_spr[bank_idx]
            } else {
                self.chr_banks_bg[bank_idx & 0x03]
            }
        } else if self.last_chr_write == ChrBank::Spr {
            self.chr_banks_spr[bank_idx]
        } else {
            self.chr_banks_bg[bank_idx & 0x03]
        };
        bank * bank_size + offset
    }

    fn nametable_mode(&self, addr: u16) -> u16 {
        let table_size = 0x0400;
        let addr = (addr - 0x2000) % 0x1000 as u16;
        let table = addr / table_size;
        u16::from((self.regs.nametable_mirroring >> (2 * table)) & 0x03)
    }
}

impl Mapper for Exrom {
    fn irq_pending(&mut self) -> bool {
        self.regs.irq_enabled && self.irq_pending
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn vram_change(&mut self, addr: u16) {
        if addr < 0x3F00 {
            self.spr_fetch_count += 1;
            if (addr >> 12) == 0x02 && addr == self.ppu_prev_addr {
                self.ppu_prev_match += 1;
                if self.ppu_prev_match == 2 {
                    if !self.regs.in_frame {
                        self.regs.in_frame = true;
                        self.regs.irq_counter = 0;
                    } else {
                        self.regs.irq_counter = self.regs.irq_counter.wrapping_add(1);
                        if self.regs.irq_counter == self.regs.scanline_num_irq {
                            self.irq_pending = true;
                        }
                    }
                    self.spr_fetch_count = 0;
                }
            } else {
                self.ppu_prev_match = 0;
            }
            self.ppu_prev_addr = addr;
            self.ppu_reading = true;
        }
    }

    fn use_ciram(&self, addr: u16) -> bool {
        let mode = self.nametable_mode(addr);
        match mode {
            0 | 1 => true,
            _ => false,
        }
    }

    fn nametable_addr(&self, addr: u16) -> u16 {
        let mode = self.nametable_mode(addr);
        match mode {
            0 | 1 => {
                let table_size = 0x0400;
                let offset = addr % table_size;
                0x2000 + mode * table_size + offset
            }
            _ => 0,
        }
    }

    fn ppu_write(&mut self, addr: u16, val: u8) {
        match addr {
            0x2000 => {
                self.regs.sprite8x16 = val & 0x20 > 0;
            }
            0x2001 => {
                self.ppu_rendering = val & 0x18 > 0; // 1, 2, or 3
                if !self.ppu_rendering {
                    self.regs.in_frame = false;
                }
            }
            0x2002 => self.ppu_in_vblank = val & 0x80 > 0,
            _ => (),
        }
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Exrom {
    fn read(&mut self, addr: u16) -> u8 {
        let val = self.peek(addr);
        match addr {
            0x5204 => {
                // Reading from IRQ status clears it
                self.irq_pending = false;
            }
            0xFFFA | 0xFFFB => {
                self.regs.in_frame = false;
            }
            _ => (),
        }
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let addr = self.get_chr_addr(addr);
                self.cart.chr_rom.peekw(addr)
            }
            0x2000..=0x3EFF => {
                let mode = self.nametable_mode(addr);
                let offset = addr % 0x0400;
                match mode {
                    2 => self.exram.peek(offset),
                    3 => {
                        if offset < 0x03C0 {
                            self.regs.fill_tile
                        } else {
                            self.regs.fill_attr
                        }
                    }
                    _ => 0,
                }
            }
            0x6000..=0xFFFF => {
                let (prg_addr, rom_select) = self.get_prg_addr(addr);
                debug!(
                    self,
                    "read addr ${:04X} -> ${:04X}, rom: {}", addr, prg_addr, rom_select
                );
                if rom_select {
                    self.cart.prg_rom.peekw(prg_addr)
                } else {
                    self.prg_ram.peekw(prg_addr)
                }
            }
            0x5C00..=0x5FFF => {
                // Modes 0-1 are nametable/attr modes and not used for RAM, thus are not readable
                if self.regs.exram_mode < 2 {
                    self.open_bus
                } else {
                    self.exram.peek(addr - 0x5C00)
                }
            }
            0x5113..=0x5117 => 0, // TODO read prg_bank?
            0x5120..=0x5127 => self.chr_banks_spr[addr as usize - 0x5120] as u8,
            0x5128..=0x512B => self.chr_banks_bg[addr as usize - 0x5128] as u8,
            0x5000..=0x5003 => 0, // TODO Sound Pulse 1
            0x5004..=0x5007 => 0, // TODO Sound Pulse 2
            0x5010..=0x5011 => 0, // TODO Sound PCM
            0x5015 => 0,          // TODO Sound General
            0x5100 => self.regs.prg_mode,
            0x5101 => self.regs.chr_mode,
            0x5130 => self.regs.chr_hi_bit,
            0x5104 => self.regs.exram_mode,
            0x5105 => self.regs.nametable_mirroring,
            0x5106 => self.regs.fill_tile,
            0x5107 => self.regs.fill_attr,
            0x5200 => self.regs.vertical_split_mode,
            0x5201 => self.regs.vertical_split_scroll,
            0x5202 => self.regs.vertical_split_bank,
            0x5203 => self.regs.scanline_num_irq as u8,
            0x5204 => (self.irq_pending as u8) << 7 | (self.regs.in_frame as u8) << 6,
            0x5205 => (self.regs.mult_result & 0xFF) as u8,
            0x5206 => ((self.regs.mult_result >> 8) & 0xFF) as u8,
            0x5207 => self.open_bus, // TODO MMC5A only CL3 / SL3 Data Direction and Output Data Source
            0x5208 => self.open_bus, // TODO MMC5A only CL3 / SL3 Status
            0x5209 => self.open_bus, // TODO MMC5A only 6-bit Hardware Timer with IRQ
            0x5800..=0x5BFF => self.open_bus, // MMC5A unknown - reads open_bus
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Exrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x2000..=0x3EFF => {
                let mode = self.nametable_mode(addr);
                if mode == 2 && self.regs.exram_mode == 0x02 {
                    self.exram.write(addr - 0x2000, val);
                }
            }
            0x6000..=0xDFFF => {
                let (prg_addr, rom_select) = self.get_prg_addr(addr);
                debug!(
                    self,
                    "write addr ${:04X} -> ${:04X}, rom: {}", addr, prg_addr, rom_select
                );
                if !rom_select && self.regs.prg_ram_protect_a && self.regs.prg_ram_protect_b {
                    self.prg_ram.writew(prg_addr, val);
                }
            }
            // [DDCC BBAA]
            //
            // Allows each Nametable slot to be configured:
            //   [   A   ][   B   ]
            //   [   C   ][   D   ]
            //
            // Values can be the following:
            //   %00 = NES internal NTA
            //   %01 = NES internal NTB
            //   %10 = use ExRAM as NT
            //   %11 = Fill Mode
            //
            // For example... some typical mirroring setups would be:
            //                        (  D  C  B  A)
            //   Horizontal:     $50  (%01 01 00 00)
            //   Vertical:       $44  (%01 00 01 00)
            //   SingleScreenA:  $00  (%00 00 00 00)
            //   SingleScreenB:  $55  (%01 01 01 01)
            //   Fill:           $ff  (%11 11 11 11)
            0x5105 => {
                self.regs.nametable_mirroring = val;
                self.mirroring = match self.regs.nametable_mirroring {
                    0x50 => Mirroring::Horizontal,
                    0x44 => Mirroring::Vertical,
                    0x00 => Mirroring::SingleScreenA,
                    0x55 => Mirroring::SingleScreenB,
                    _ => Mirroring::FourScreen,
                };
            }
            // 'A' Chr Regs
            0x5120..=0x5127 => {
                self.last_chr_write = ChrBank::Spr;
                self.chr_banks_spr[addr as usize - 0x5120] =
                    val as usize | (self.regs.chr_hi_bit as usize) << 8;
            }
            // 'B' Chr Regs
            0x5128..=0x512B => {
                self.last_chr_write = ChrBank::Bg;
                self.chr_banks_bg[addr as usize - 0x5128] =
                    val as usize | (self.regs.chr_hi_bit as usize) << 8;
            }
            // PRG Bank Switching
            // $5113: [.... .PPP]
            //      8k PRG-RAM @ $6000
            // $5114-5117: [RPPP PPPP]
            //      R = ROM select (0=select RAM, 1=select ROM)  **unused in $5117**
            //      P = PRG page
            0x5113..=0x5117 => {
                self.prg_banks[addr as usize - 0x5113] = val as usize;
                debug!(self, "set prg bank {} - ${:02X}", addr - 0x5113, val);
            }
            0x5C00..=0x5FFF => {
                // Mode 2 is writable
                if self.regs.exram_mode == 0x02 {
                    self.exram.write(addr - 0x5C00, val);
                }
            }
            0x5000..=0x5003 => (), // TODO Sound Pulse 1
            0x5004..=0x5007 => (), // TODO Sound Pulse 2
            0x5010..=0x5011 => (), // TODO Sound PCM
            0x5015 => (),          // TODO Sound General
            // [.... ..PP]    PRG Mode
            //      %00 = 32k
            //      %01 = 16k
            //      %10 = 16k+8k
            //      %11 = 8k
            0x5100 => self.regs.prg_mode = val & 0x03,
            // [.... ..CC]    CHR Mode
            //      %00 = 8k Mode
            //      %01 = 4k Mode
            //      %10 = 2k Mode
            //      %11 = 1k Mode
            0x5101 => self.regs.chr_mode = val & 0x03,
            // [.... ..HH]
            0x5130 => self.regs.chr_hi_bit = val & 0x03,
            // [.... ..AA]    PRG-RAM Protect A
            // [.... ..BB]    PRG-RAM Protect B
            //      To allow writing to PRG-RAM you must set these regs to the following values:
            //         A=%10
            //         B=%01
            //      Any other values will prevent PRG-RAM writing.
            0x5102 => self.regs.prg_ram_protect_a = (val & 0x03) == 0b10,
            0x5103 => self.regs.prg_ram_protect_b = (val & 0x03) == 0b01,
            // [.... ..XX]    ExRAM mode
            //     %00 = Extra Nametable mode    ("Ex0")
            //     %01 = Extended Attribute mode ("Ex1")
            //     %10 = CPU access mode         ("Ex2")
            //     %11 = CPU read-only mode      ("Ex3")
            0x5104 => self.regs.exram_mode = val & 0x03,
            // [TTTT TTTT]     Fill Tile
            0x5106 => self.regs.fill_tile = val,
            // [.... ..AA]     Fill Attribute bits
            0x5107 => self.regs.fill_attr = val & 0x03,
            0x5200 => {
                self.regs.vertical_split_mode = val;
            }
            0x5201 => self.regs.vertical_split_scroll = val,
            0x5202 => self.regs.vertical_split_bank = val,
            0x5203 => self.regs.scanline_num_irq = u16::from(val),
            0x5204 => self.regs.irq_enabled = val & 0x80 > 0,
            0x5205 => self.regs.multiplicand = val,
            0x5206 => self.regs.mult_result = u16::from(self.regs.multiplicand) * u16::from(val),
            0x5207 => (), // TODO MMC5A only CL3 / SL3 Data Direction and Output Data Source
            0x5208 => (), // TODO MMC5A only CL3 / SL3 Status
            0x5209 => (), // TODO MMC5A only 6-bit Hardware Timer with IRQ
            0x5800..=0x5BFF => (), // MMC5A unknown
            0x0000..=0x1FFF => (), // ROM is write-only
            0xE000..=0xFFFF => (), // ROM is write-only
            _ => (),
        }
    }
}

impl Clocked for Exrom {
    fn clock(&mut self) -> usize {
        if self.ppu_reading {
            self.ppu_idle = 0;
        } else {
            self.ppu_idle += 1;
            if self.ppu_idle == 9 {
                // 3 CPU clocks == 9 Mapper clocks
                self.ppu_idle = 0;
                self.regs.in_frame = false;
            }
        }
        self.ppu_reading = false;
        1
    }
}

impl Powered for Exrom {
    fn reset(&mut self) {
        self.regs.prg_mode = 0x03;
        self.regs.chr_mode = 0x03;
    }
}

impl Loggable for Exrom {
    fn set_log_level(&mut self, level: LogLevel) {
        self.log_level = level;
    }
    fn log_level(&self) -> LogLevel {
        self.log_level
    }
}

impl Savable for Exrom {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.regs.save(fh)?;
        self.mirroring.save(fh)?;
        self.irq_pending.save(fh)?;
        self.last_chr_write.save(fh)?;
        self.spr_fetch_count.save(fh)?;
        self.ppu_prev_addr.save(fh)?;
        self.ppu_prev_match.save(fh)?;
        self.ppu_reading.save(fh)?;
        self.ppu_idle.save(fh)?;
        self.ppu_in_vblank.save(fh)?;
        self.ppu_cycle.save(fh)?;
        self.ppu_rendering.save(fh)?;
        self.prg_banks.save(fh)?;
        self.chr_banks_spr.save(fh)?;
        self.chr_banks_bg.save(fh)?;
        self.cart.save(fh)?;
        self.prg_ram.save(fh)?;
        self.exram.save(fh)?;
        self.open_bus.save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.regs.load(fh)?;
        self.mirroring.load(fh)?;
        self.irq_pending.load(fh)?;
        self.last_chr_write.load(fh)?;
        self.spr_fetch_count.load(fh)?;
        self.ppu_prev_addr.load(fh)?;
        self.ppu_prev_match.load(fh)?;
        self.ppu_reading.load(fh)?;
        self.ppu_idle.load(fh)?;
        self.ppu_in_vblank.load(fh)?;
        self.ppu_cycle.load(fh)?;
        self.ppu_rendering.load(fh)?;
        self.prg_banks.load(fh)?;
        self.chr_banks_spr.load(fh)?;
        self.chr_banks_bg.load(fh)?;
        self.cart.load(fh)?;
        self.prg_ram.load(fh)?;
        self.exram.load(fh)?;
        self.open_bus.load(fh)?;
        Ok(())
    }
}

impl Savable for ExRegs {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.sprite8x16.save(fh)?;
        self.prg_mode.save(fh)?;
        self.chr_mode.save(fh)?;
        self.chr_hi_bit.save(fh)?;
        self.prg_ram_protect_a.save(fh)?;
        self.prg_ram_protect_b.save(fh)?;
        self.exram_mode.save(fh)?;
        self.nametable_mirroring.save(fh)?;
        self.fill_tile.save(fh)?;
        self.fill_attr.save(fh)?;
        self.vertical_split_mode.save(fh)?;
        self.vertical_split_scroll.save(fh)?;
        self.vertical_split_bank.save(fh)?;
        self.scanline_num_irq.save(fh)?;
        self.irq_enabled.save(fh)?;
        self.irq_counter.save(fh)?;
        self.in_frame.save(fh)?;
        self.multiplicand.save(fh)?;
        self.multiplier.save(fh)?;
        self.mult_result.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.sprite8x16.load(fh)?;
        self.prg_mode.load(fh)?;
        self.chr_mode.load(fh)?;
        self.chr_hi_bit.load(fh)?;
        self.prg_ram_protect_a.load(fh)?;
        self.prg_ram_protect_b.load(fh)?;
        self.exram_mode.load(fh)?;
        self.nametable_mirroring.load(fh)?;
        self.fill_tile.load(fh)?;
        self.fill_attr.load(fh)?;
        self.vertical_split_mode.load(fh)?;
        self.vertical_split_scroll.load(fh)?;
        self.vertical_split_bank.load(fh)?;
        self.scanline_num_irq.load(fh)?;
        self.irq_enabled.load(fh)?;
        self.irq_counter.load(fh)?;
        self.in_frame.load(fh)?;
        self.multiplicand.load(fh)?;
        self.multiplier.load(fh)?;
        self.mult_result.load(fh)
    }
}

impl Savable for ChrBank {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => ChrBank::Spr,
            1 => ChrBank::Bg,
            _ => panic!("invalid ChrBank value"),
        };
        Ok(())
    }
}

impl fmt::Debug for Exrom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Exrom {{ }}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::Cartridge;

    #[test]
    fn prg_ram_protect() {
        for a in 0..4 {
            for b in 0..4 {
                let cart = Cartridge::new();
                let exrom = Exrom::load(cart);
                let mut exrom = exrom.borrow_mut();

                exrom.write(0x5102, a);
                exrom.write(0x5103, b);
                exrom.write(0x8000, 0xFF);
                let val = exrom.read(0x8000);
                if a == 0b10 && b == 0b01 {
                    assert_eq!(val, 0xFF, "RAM protect disabled: %{:02b}, %{:02b}", a, b);
                } else {
                    assert_eq!(val, 0x00, "RAM protect enabled: %{:02b}, %{:02b}", a, b);
                }
            }
        }
    }
}
