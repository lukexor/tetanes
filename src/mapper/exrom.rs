//! ExROM/MMC5 (Mapper 5)
//!
//! [https://wiki.nesdev.com/w/index.php/ExROM]()
//! [https://wiki.nesdev.com/w/index.php/MMC5]()

use crate::cartridge::Cartridge;
use crate::console::ppu::{Ppu, PRERENDER_SCANLINE};
use crate::mapper::Mirroring;
use crate::mapper::{Mapper, MapperRef};
use crate::memory::{Banks, Memory, Ram, Rom};
use crate::serialization::Savable;
use crate::Result;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
const PRG_RAM_SIZE: usize = 64 * 1024;
const EX_RAM_SIZE: usize = 1024;

/// ExROM
#[derive(Debug)]
pub struct Exrom {
    regs: ExRegs,
    open_bus: u8,
    irq_pending: bool,
    logging: bool,
    mirroring: Mirroring,
    battery_backed: bool,
    prg_banks: [usize; 5],
    chr_banks_spr: [usize; 8],
    chr_banks_bg: [usize; 4],
    ex_ram: Ram,
    prg_ram: Ram,
    prg_rom: Rom,
    chr: Ram,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ChrBank {
    Spr,
    Bg,
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
    chr_hi_bit: u8,
    last_chr_write: ChrBank,
    sprite8x16: bool, // $2000 PPUCTRL: false = 8x8, true = 8x16
    scanline: u16,
    sp_fetch_count: u32,
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
    irq_counter: u16,
    in_frame: bool,
    multiplicand: u8, // $5205: write
    multiplier: u8,   // $5206: write
    mult_result: u16, // $5205: read lo, $5206: read hi
}

impl Exrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let prg_ram = Ram::init(PRG_RAM_SIZE);
        let ex_ram = Ram::init(EX_RAM_SIZE);
        let num_rom_banks = cart.prg_rom.len() / (8 * 1024); // Default PRG ROM Bank size

        let mut exrom = Self {
            regs: ExRegs {
                prg_mode: 0xFF,
                chr_mode: 0xFF,
                chr_hi_bit: 0u8,
                last_chr_write: ChrBank::Spr,
                sprite8x16: false,
                scanline: 0u16,
                sp_fetch_count: 0u32,
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
                irq_counter: 0u16,
                in_frame: false,
                multiplicand: 0xFF,
                multiplier: 0xFF,
                mult_result: 0xFE01,
            },
            open_bus: 0u8,
            irq_pending: false,
            logging: false,
            mirroring: cart.mirroring(),
            battery_backed: cart.battery_backed(),
            prg_banks: [0; 5],
            chr_banks_spr: [0; 8],
            chr_banks_bg: [0; 4],
            ex_ram,
            prg_ram,
            prg_rom: cart.prg_rom,
            chr: cart.chr_rom.to_ram(),
        };
        exrom.prg_banks[3] = 0x80 | num_rom_banks - 2;
        exrom.prg_banks[4] = 0x80 | num_rom_banks - 1;
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
        // Modes 0-1 are nametable/attr modes and not used for RAM, thus are not readable
        if self.regs.extended_ram_mode < 2 {
            return self.open_bus;
        }
        if self.logging {
            eprintln!(
                "Reading ${:02X} from EX RAM: ${:04X} - mode: {}",
                self.ex_ram[addr as usize - 0x5C00],
                addr as usize - 0x5C00,
                self.regs.extended_ram_mode
            );
        }
        self.ex_ram[addr as usize - 0x5C00]
    }

    fn write_expansion_ram(&mut self, addr: u16, mut val: u8) {
        // Modes 0-2 are writable
        if self.regs.extended_ram_mode < 3 {
            // Modes 0-1 are for nametable and attributes, so write 0 if not rendering
            if self.regs.extended_ram_mode < 2 && !self.regs.rendering_enabled {
                val = 0;
            }
            if self.logging {
                eprintln!(
                    "Writing ${:02X} to EX RAM: ${:04X} - mode: {}",
                    val,
                    addr as usize - 0x5C00,
                    self.regs.extended_ram_mode
                );
            }
            self.ex_ram[addr as usize - 0x5C00] = val;
        }
    }

    fn set_mirroring(&mut self, val: u8) {
        self.regs.nametable_mapping = val;
        self.mirroring = match val {
            0x50 => Mirroring::Horizontal,
            0x44 => Mirroring::Vertical,
            0x00 => Mirroring::SingleScreen0,
            0x55 => Mirroring::SingleScreen1,
            0xAA => Mirroring::SingleScreenEx,
            0xE4 => Mirroring::FourScreen,
            0xFF => Mirroring::SingleScreenFill,
            0x14 => Mirroring::Diagonal,
            _ => {
                // $D8 = 11 01 10 00
                // 11: $2C00-$2FFF - Fill-mode
                // 01: $2800-$2BFF - nametable 1
                // 10: $2400-$27FF - nametable EX RAM or all 0s
                // 00: $2000-$23FF - nametable 0
                //
                // +-------+-------+
                // | $2000 | $2400 |
                // |   0   |  EXR  |
                // |       |       |
                // +-------+-------+
                // | $2800 | $2C00 |
                // |   1   |  FIL  |
                // |       |       |
                // +-------+-------+
                eprintln!("impossible mirroring mode: ${:02X}", val);
                self.mirroring
            }
        };
        if self.logging {
            println!("{:?}", self.mirroring);
        }
    }

    fn clock_irq(&mut self) {
        // not in-frame
        if self.logging {
            println!(
                "scanline: {}, scanline irq: {}, irq counter: {}, in_frame: {}",
                self.regs.scanline,
                self.regs.scanline_num_irq,
                self.regs.irq_counter,
                self.regs.in_frame
            );
        }
        if !self.regs.in_frame {
            if self.logging {
                println!("irq reset");
            }
            self.regs.in_frame = true;
            self.irq_pending = false;
            self.regs.irq_counter = 0;
        } else {
            self.regs.irq_counter = self.regs.irq_counter.wrapping_add(1);
            if self.regs.irq_counter == u16::from(self.regs.scanline_num_irq) {
                if self.logging {
                    println!("irq triggered {}", self.regs.scanline_num_irq);
                }
                self.irq_pending = true;
                // self.regs.irq_counter = 0;
            }
        }
    }
}

impl Mapper for Exrom {
    fn irq_pending(&mut self) -> bool {
        if self.regs.irq_enabled {
            let irq = self.irq_pending;
            self.irq_pending = false;
            irq
        } else {
            false
        }
    }
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn vram_change(&mut self, _ppu: &Ppu, addr: u16) {
        if addr <= 0x1FFF {
            self.regs.sp_fetch_count += 1;
        }
    }
    fn clock(&mut self, ppu: &Ppu) {
        if ppu.vblank_started() || ppu.scanline == PRERENDER_SCANLINE {
            self.regs.in_frame = false;
        }

        if self.regs.scanline != ppu.scanline {
            self.clock_irq();
            self.regs.sp_fetch_count = 0;
            self.regs.scanline = ppu.scanline;
        }

        self.regs.sprite8x16 = ppu.regs.ctrl.sprite_height() == 16;
        self.regs.rendering_enabled = ppu.rendering_enabled();
    }
    fn battery_backed(&self) -> bool {
        false
    }
    fn save_sram(&self, _fh: &mut dyn Write) -> Result<()> {
        Ok(())
    }
    fn load_sram(&mut self, _fh: &mut dyn Read) -> Result<()> {
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
    fn set_logging(&mut self, logging: bool) {
        self.logging = logging;
    }
}

impl Memory for Exrom {
    fn read(&mut self, addr: u16) -> u8 {
        let val = self.peek(addr);
        if addr == 0x5204 {
            // Reading from IRQ status clears it
            self.irq_pending = false;
        }
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let mut bank_size = 1;
                let mut bank_idx_a = ((addr >> 10) & 0x0F) as usize;
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
                    if self.regs.sp_fetch_count >= 32 && self.regs.sp_fetch_count < 40 {
                        self.chr_banks_spr[bank_idx_a]
                    } else {
                        self.chr_banks_bg[bank_idx_b]
                    }
                } else if self.regs.last_chr_write == ChrBank::Spr {
                    self.chr_banks_spr[bank_idx_a]
                } else {
                    self.chr_banks_bg[bank_idx_b]
                };
                let offset = addr as usize % bank_size;
                self.chr[bank * bank_size + offset]
            }
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
            0x5C00..=0x5FFF => self.read_expansion_ram(addr),
            0x5113..=0x5117 => 0, // TODO read prg_bank?
            0x5120..=0x512B => 0, // TODO read chr_bank?
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
            0x5130 => self.regs.chr_hi_bit,
            0x5200 => self.regs.vertical_split_mode,
            0x5201 => self.regs.vertical_split_scroll,
            0x5202 => self.regs.vertical_split_bank,
            0x5203 => self.regs.scanline_num_irq,
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

    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        match addr {
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
            0x5105 => self.set_mirroring(val),
            0x5120..=0x5127 => {
                self.regs.last_chr_write = ChrBank::Spr;
                self.chr_banks_spr[(addr & 0x07) as usize] =
                    val as usize | (self.regs.chr_hi_bit as usize) << 8;
            }
            0x5128..=0x512B => {
                self.regs.last_chr_write = ChrBank::Bg;
                self.chr_banks_bg[(addr & 0x03) as usize] =
                    val as usize | (self.regs.chr_hi_bit as usize) << 8;
            }
            0x5113..=0x5117 => self.write_prg_bankswitching(addr, val),
            0x5C00..=0x5FFF => self.write_expansion_ram(addr, val),
            0x5130 => self.regs.chr_hi_bit = val & 0x3,
            0x5000..=0x5003 => (), // TODO Sound Pulse 1
            0x5004..=0x5007 => (), // TODO Sound Pulse 2
            0x5010..=0x5011 => (), // TODO Sound PCM
            0x5015 => (),          // TODO Sound General
            0x5100 => self.regs.prg_mode = val & 0x03,
            0x5101 => self.regs.chr_mode = val & 0x03,
            0x5102 => self.regs.prg_ram_protect1 = val & 0x03,
            0x5103 => self.regs.prg_ram_protect2 = val & 0x03,
            0x5104 => self.regs.extended_ram_mode = val & 0x03,
            0x5106 => self.regs.fill_tile = val,
            0x5107 => self.regs.fill_attr = val & 0x03,
            0x5200 => self.regs.vertical_split_mode = val,
            0x5201 => self.regs.vertical_split_scroll = (val >> 3) & 0x1F,
            0x5202 => self.regs.vertical_split_bank = val & 0x3F,
            0x5203 => self.regs.scanline_num_irq = val,
            0x5204 => self.regs.irq_enabled = val & 0x80 == 0x80,
            0x5205 => self.regs.multiplicand = val,
            0x5206 => self.multiplier(val),
            0x5207 => (), // TODO MMC5A only CL3 / SL3 Data Direction and Output Data Source
            0x5208 => (), // TODO MMC5A only CL3 / SL3 Status
            0x5209 => (), // TODO MMC5A only 6-bit Hardware Timer with IRQ
            0x5800..=0x5BFF => (), // MMC5A unknown
            0x0000..=0x1FFF => (), // ROM is write-only
            0xE000..=0xFFFF => (), // ROM is write-only
            _ => (),
        }
    }

    fn reset(&mut self) {
        self.regs.prg_mode = 0xFF;
        self.regs.chr_mode = 0xFF;
    }
    fn power_cycle(&mut self) {
        self.reset();
    }
}

impl Savable for Exrom {
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        self.regs.save(fh)?;
        self.open_bus.save(fh)?;
        self.irq_pending.save(fh)?;
        self.logging.save(fh)?;
        self.mirroring.save(fh)?;
        self.battery_backed.save(fh)?;
        self.prg_banks.save(fh)?;
        self.chr_banks_spr.save(fh)?;
        self.chr_banks_bg.save(fh)?;
        self.ex_ram.save(fh)?;
        self.prg_ram.save(fh)?;
        self.prg_rom.save(fh)?;
        self.chr.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
        self.regs.load(fh)?;
        self.open_bus.load(fh)?;
        self.irq_pending.load(fh)?;
        self.logging.load(fh)?;
        self.mirroring.load(fh)?;
        self.battery_backed.load(fh)?;
        self.prg_banks.load(fh)?;
        self.chr_banks_spr.load(fh)?;
        self.chr_banks_bg.load(fh)?;
        self.ex_ram.load(fh)?;
        self.prg_ram.load(fh)?;
        self.prg_rom.load(fh)?;
        self.chr.load(fh)
    }
}

impl Savable for ExRegs {
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        self.prg_mode.save(fh)?;
        self.chr_mode.save(fh)?;
        self.chr_hi_bit.save(fh)?;
        self.last_chr_write.save(fh)?;
        self.sprite8x16.save(fh)?;
        self.scanline.save(fh)?;
        self.sp_fetch_count.save(fh)?;
        self.rendering_enabled.save(fh)?;
        self.prg_ram_protect1.save(fh)?;
        self.prg_ram_protect2.save(fh)?;
        self.extended_ram_mode.save(fh)?;
        self.nametable_mapping.save(fh)?;
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
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
        self.prg_mode.load(fh)?;
        self.chr_mode.load(fh)?;
        self.chr_hi_bit.load(fh)?;
        self.last_chr_write.load(fh)?;
        self.sprite8x16.load(fh)?;
        self.scanline.load(fh)?;
        self.sp_fetch_count.load(fh)?;
        self.rendering_enabled.load(fh)?;
        self.prg_ram_protect1.load(fh)?;
        self.prg_ram_protect2.load(fh)?;
        self.extended_ram_mode.load(fh)?;
        self.nametable_mapping.load(fh)?;
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
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
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
