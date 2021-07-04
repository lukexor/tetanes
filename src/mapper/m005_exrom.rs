//! ExROM/MMC5 (Mapper 5)
//!
//! [https://wiki.nesdev.com/w/index.php/ExROM]()
//! [https://wiki.nesdev.com/w/index.php/MMC5]()

use crate::{
    apu::{
        dmc::Dmc,
        pulse::{Pulse, PulseChannel},
    },
    cartridge::Cartridge,
    common::{Addr, Clocked, Powered},
    mapper::{Mapper, MapperType, Mirroring},
    memory::{BankedMemory, MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

const PRG_WINDOW: usize = 8 * 1024;
const PRG_RAM_SIZE: usize = 64 * 1024; // Easier to just provide 64L since mappers don't always specify
const EXRAM_WINDOW: usize = 1024;
const EXRAM_SIZE: usize = 1024;
const CHR_ROM_WINDOW: usize = 1024;
const ATTR_BITS: [u8; 4] = [0x00, 0x55, 0xAA, 0xFF];
const ATTR_LOC: [u8; 256] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
    0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
    0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
    0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F,
    0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37,
    0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F,
    0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F,
];
const ATTR_SHIFT: [u8; 128] = [
    0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2,
    0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2,
    4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6,
    4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6,
];

/// ExROM
#[derive(Debug, Clone)]
pub struct Exrom {
    regs: ExRegs,
    mirroring: Mirroring,
    irq_pending: bool,
    spr_fetch_count: u32,
    ppu_prev_addr: u16,
    ppu_prev_match: u8,
    ppu_reading: bool,
    ppu_idle: u8,
    ppu_in_vblank: bool,
    ppu_rendering: bool,
    prg_ram: BankedMemory,
    exram: BankedMemory,
    prg_rom: BankedMemory,
    chr_rom: BankedMemory,
    tile_cache: u16,
    in_split: bool,
    split_tile: u16,
    pulse1: Pulse,
    pulse2: Pulse,
    dmc: Dmc,
    dmc_mode: u8,
    open_bus: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum PrgMode {
    Bank32k,
    Bank16k,
    Bank16_8k,
    Bank8k,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ChrBank {
    Spr,
    Bg,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ChrMode {
    Bank8k,
    Bank4k,
    Bank2k,
    Bank1k,
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Nametable {
    NTA,
    NTB,
    ExRAM,
    Fill,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ExRamMode {
    Nametable,
    ExAttr,
    Ram,
    RamProtected,
}

#[derive(Debug, Clone)]
struct ExRegs {
    sprite8x16: bool,         // $2000 PPUCTRL: false = 8x8, true = 8x16
    prg_mode: PrgMode,        // $5100
    chr_mode: ChrMode,        // $5101
    prg_ram_protect: [u8; 2], // $5102 & $5103
    exram_mode: ExRamMode,    // $5104
    nametable_mirroring: u8,  // $5105
    fill_tile: u8,            // $5106
    fill_attr: u8,            // $5107
    prg_banks: [usize; 5],    // $5113 - $5117
    chr_banks: [usize; 16],   // $5120 - $512B
    last_chr_write: ChrBank,
    chr_hi: usize,         // $5130
    vsplit_enabled: bool,  // $5200 [E... ....]
    vsplit_side: Split,    // $5200 [.S.. ....]
    vsplit_tile: u8,       // $5200 [...T TTTT]
    vsplit_scroll: u8,     // $5201
    vsplit_bank: u8,       // $5202
    scanline_num_irq: u16, // $5203: Write $00 to disable IRQs
    irq_enabled: bool,     // $5204
    irq_counter: u16,
    in_frame: bool,
    multiplicand: u8, // $5205: write
    multiplier: u8,   // $5206: write
    mult_result: u16, // $5205: read lo, $5206: read hi
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Split {
    Left,
    Right,
}

impl ExRegs {
    fn new(mirroring: Mirroring) -> Self {
        Self {
            sprite8x16: false,
            prg_mode: PrgMode::Bank8k,
            chr_mode: ChrMode::Bank1k,
            prg_ram_protect: [0x00; 2],
            exram_mode: ExRamMode::RamProtected,
            nametable_mirroring: mirroring.into(),
            fill_tile: 0xFF,
            fill_attr: 0xFF,
            prg_banks: [0x00; 5],
            chr_banks: [0x00; 16],
            last_chr_write: ChrBank::Spr,
            chr_hi: 0x00,
            vsplit_enabled: false,
            vsplit_side: Split::Left,
            vsplit_tile: 0x00,
            vsplit_scroll: 0x00,
            vsplit_bank: 0x00,
            scanline_num_irq: 0x00,
            irq_enabled: false,
            irq_counter: 0x0000,
            in_frame: false,
            multiplicand: 0xFF,
            multiplier: 0xFF,
            mult_result: 0xFE01, // e.g. 0xFF * 0xFF
        }
    }
}

impl Exrom {
    pub fn load(cart: Cartridge, consistent_ram: bool) -> MapperType {
        let mirroring = cart.mirroring();
        let mut exrom = Self {
            regs: ExRegs::new(mirroring),
            mirroring,
            irq_pending: false,
            spr_fetch_count: 0x00,
            ppu_prev_addr: 0xFFFF,
            ppu_prev_match: 0x0000,
            ppu_reading: false,
            ppu_idle: 0x00,
            ppu_in_vblank: false,
            ppu_rendering: false,
            prg_ram: BankedMemory::ram(PRG_RAM_SIZE, PRG_WINDOW, consistent_ram),
            exram: BankedMemory::ram(EXRAM_SIZE, EXRAM_WINDOW, consistent_ram),
            prg_rom: BankedMemory::from(cart.prg_rom, PRG_WINDOW),
            chr_rom: BankedMemory::from(cart.chr_rom, CHR_ROM_WINDOW),
            tile_cache: 0x0000,
            in_split: false,
            split_tile: 0x0000,
            pulse1: Pulse::new(PulseChannel::One),
            pulse2: Pulse::new(PulseChannel::Two),
            dmc: Dmc::new(),
            dmc_mode: 0x01, // Default to read mode
            open_bus: 0x00,
        };
        exrom.prg_ram.add_bank_range(0x6000, 0xFFFF);
        exrom.exram.add_bank(0x0000, 0x0400);
        exrom.prg_rom.add_bank_range(0x8000, 0xFFFF);
        exrom.regs.prg_banks[4] = exrom.prg_rom.last_bank() | 0x80;
        exrom.update_prg_banks();
        exrom.chr_rom.add_bank_range(0x0000, 0x1FFF);
        exrom.into()
    }

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
    fn update_prg_banks(&mut self) {
        use PrgMode::*;

        let mode = self.regs.prg_mode;
        let banks = self.regs.prg_banks;

        // $5113 always selects RAM
        self.set_prg_bank_range(0x6000, 0x7FFF, banks[0], false);
        match mode {
            // $5117 always selects ROM
            Bank32k => self.set_prg_bank_range(0x8000, 0xFFFF, banks[4], true),
            Bank16k => {
                let rom = banks[2] & 0x80 > 0;
                self.set_prg_bank_range(0x8000, 0xBFFF, banks[2], rom);
                // $5117 always selets ROM
                self.set_prg_bank_range(0xC000, 0xFFFF, banks[4], true);
            }
            Bank16_8k => {
                let rom = banks[2] & 0x80 > 0;
                self.set_prg_bank_range(0x8000, 0xBFFF, banks[2], rom);
                let rom = banks[3] & 0x80 > 0;
                self.set_prg_bank_range(0xC000, 0xDFFF, banks[3], rom);
                // $5117 always selets ROM
                self.set_prg_bank_range(0xE000, 0xFFFF, banks[4], true);
            }
            Bank8k => {
                for (i, bank) in banks[1..5].iter().enumerate() {
                    // $5116 always selects ROM
                    let rom = if i == 4 { false } else { bank & 0x80 > 0 };
                    let start = 0x8000 + i as Addr * 0x2000;
                    let end = start + 0x1FFF;
                    self.set_prg_bank_range(start, end, *bank, rom);
                }
            }
        };
    }

    fn set_prg_bank_range(&mut self, start: Addr, end: Addr, bank: usize, rom: bool) {
        let bank = bank & 0x7F;
        if rom {
            self.prg_rom.set_bank_range(start, end, bank);
        } else {
            self.prg_ram.set_bank_range(start, end, bank);
        }
    }

    // Maps an address to a given PRG Bank Register based on the current PRG MODE
    // Returns the bank page number and the ROM select bit
    fn prg_addr_bank(&self, addr: Addr) -> (usize, bool) {
        let mode = self.regs.prg_mode;
        let banks = self.regs.prg_banks;
        use PrgMode::*;
        let bank = match addr {
            0x6000..=0x7FFF => banks[0],
            0x8000..=0x9FFF => match mode {
                Bank8k => banks[1],
                Bank16k | Bank16_8k => banks[2],
                Bank32k => banks[4],
            },
            0xA000..=0xBFFF => match mode {
                Bank8k | Bank16k | Bank16_8k => banks[2],
                Bank32k => banks[4],
            },
            0xC000..=0xDFFF => match mode {
                Bank8k | Bank16_8k => banks[3],
                Bank16k | Bank32k => banks[4],
            },
            0xE000..=0xFFFF => banks[4],
            _ => 0x00,
        };
        let rom_select = match addr {
            0x6000..=0x7FFF => false,
            _ => match mode {
                PrgMode::Bank32k => true,
                _ => bank & 0x80 > 0,
            },
        };
        // NOTE: Bank numbers 2 and 4 normally are right shifted to the correct page size
        // but because we use the smallest bank size by default and set_bank_range,
        // this becomes unnecessary
        (bank & 0x7F, rom_select)
    }

    fn rom_select(&self, addr: Addr) -> bool {
        let (_, rom_select) = self.prg_addr_bank(addr);
        rom_select
    }

    // 'A' Set (Sprites):
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
    #[allow(clippy::needless_range_loop)]
    fn update_chr_banks(&mut self, chr_bank: ChrBank) {
        let banks = match chr_bank {
            ChrBank::Spr => &self.regs.chr_banks[0..8],
            ChrBank::Bg => &self.regs.chr_banks[8..16],
        };
        // CHR banks are in actual page sizes which means they need to be shifted appropriately
        match self.regs.chr_mode {
            ChrMode::Bank8k => self.chr_rom.set_bank_range(0x0000, 0x1FFF, banks[7] << 3),
            ChrMode::Bank4k => {
                self.chr_rom.set_bank_range(0x0000, 0x0FFF, banks[3] << 2);
                self.chr_rom.set_bank_range(0x1000, 0x1FFF, banks[7] << 2);
            }
            ChrMode::Bank2k => {
                self.chr_rom.set_bank_range(0x0000, 0x07FF, banks[1] << 1);
                self.chr_rom.set_bank_range(0x0800, 0x0FFF, banks[3] << 1);
                self.chr_rom.set_bank_range(0x1000, 0x17FF, banks[5] << 1);
                self.chr_rom.set_bank_range(0x1800, 0x1FFF, banks[7] << 1);
            }
            ChrMode::Bank1k => {
                for (i, bank) in banks[0..8].iter().enumerate() {
                    let start = i as Addr * 0x0400;
                    let end = start + 0x03FF;
                    self.chr_rom.set_bank_range(start, end, *bank);
                }
            }
        };
    }

    // Determine the nametable we're trying to access
    fn nametable_mapping(&self, addr: u16) -> Nametable {
        let table_size = 0x0400;
        let addr = (addr - 0x2000) % 0x1000;
        let table = addr / table_size;
        match (self.regs.nametable_mirroring >> (2 * table)) & 0x03 {
            0 => Nametable::NTA,
            1 => Nametable::NTB,
            2 => Nametable::ExRAM,
            3 => Nametable::Fill,
            _ => panic!("invalid mirroring"),
        }
    }

    fn update_ram_protection(&mut self) {
        // To allow writing to PRG-RAM you must set:
        //    A=%10
        //    B=%01
        // Any other value will prevent PRG-RAM writing.
        let writable = self.regs.prg_ram_protect[0] == 0b10 && self.regs.prg_ram_protect[1] == 0b01;
        self.prg_ram.write_protect(!writable);
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

            if self.regs.sprite8x16 {
                match self.spr_fetch_count {
                    127 => self.update_chr_banks(ChrBank::Spr),
                    160 => self.update_chr_banks(ChrBank::Bg),
                    _ => (),
                }
            }

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

        if self.regs.exram_mode == ExRamMode::ExAttr
            && addr >= 0x2000
            && addr <= 0x3EFF
            && (addr % 0x0400) < 0x3C0
            && (self.spr_fetch_count < 127 || self.spr_fetch_count > 159)
        {
            self.tile_cache = addr % 0x0400;
        }
    }

    // Used by the PPU to determine whether it should use it's own internal CIRAM for nametable
    // reads or to read CIRAM instead from the mapper
    fn use_ciram(&self, addr: u16) -> bool {
        if self.in_split
            || (self.regs.exram_mode == ExRamMode::ExAttr
                && (addr % 0x0400) >= 0x3C0
                && (self.spr_fetch_count < 127 || self.spr_fetch_count > 159))
        {
            // If we're in Extended Attribute mode and reading BG attributes,
            // yield to mapper for Attribute data instead of PPU
            false
        } else {
            // 0 and 1 mean NametableA and NametableB
            // 2 means internal EXRAM
            // 3 means Fill-mode
            let nametable = self.nametable_mapping(addr);
            matches!(nametable, Nametable::NTA | Nametable::NTB)
        }
    }

    // Returns a nametable page based on $5105 nametable mapping
    // 0/1 use PPU CIRAM, 2/3 use EXRAM/Fill-mode
    fn nametable_page(&self, addr: u16) -> u16 {
        let nametable = self.nametable_mapping(addr);
        match nametable {
            Nametable::NTA | Nametable::NTB => nametable as u16,
            _ => 0,
        }
    }

    fn ppu_write(&mut self, addr: u16, val: u8) {
        match addr {
            0x2000 => self.regs.sprite8x16 = val & 0x20 > 0,
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
            0x2000..=0x3EFF => {
                let offset = addr % 0x0400;
                if self.in_split && offset < 0x03C0 {
                    self.split_tile = (u16::from(self.regs.vsplit_scroll & 0xF8) << 2)
                        | ((self.spr_fetch_count / 4) & 0x1F) as u16;
                }
            }
            0x5204 => {
                // Reading from IRQ status clears it
                self.irq_pending = false;
            }
            0x5010 => {
                self.dmc.irq_pending = false;
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
                if self.regs.exram_mode == ExRamMode::ExAttr
                    && (self.spr_fetch_count < 127 || self.spr_fetch_count > 159)
                {
                    let hibits = self.regs.chr_hi << 18;
                    let exbits = (self.exram.peek(self.tile_cache) as usize & 0x3F) << 12;
                    let mut addr = hibits | exbits | (addr as usize) & 0x0FFF;
                    if addr >= self.chr_rom.len() {
                        addr %= self.chr_rom.len();
                    }
                    self.chr_rom[addr]
                } else {
                    self.chr_rom.peek(addr)
                }
            }
            0x2000..=0x3EFF => {
                let offset = addr % 0x0400;
                if self.in_split {
                    if offset < 0x03C0 {
                        self.exram.peek(self.split_tile)
                    } else {
                        let addr = 0x03C0 | u16::from(ATTR_LOC[(self.split_tile as usize) >> 2]);
                        let attr = self.exram.peek(addr) as usize;
                        let shift = ATTR_SHIFT[(self.split_tile as usize) & 0x7F] as usize;
                        ATTR_BITS[(attr >> shift) & 0x03]
                    }
                } else {
                    let nametable = self.nametable_mapping(addr);
                    match self.regs.exram_mode {
                        ExRamMode::ExAttr if offset >= 0x03C0 => {
                            ATTR_BITS[(self.exram.peek(self.tile_cache) as usize >> 6) & 0x03]
                        }
                        ExRamMode::Nametable | ExRamMode::ExAttr
                            if nametable == Nametable::ExRAM =>
                        {
                            self.exram.peek(addr - 0x2000)
                        }
                        ExRamMode::Nametable | ExRamMode::ExAttr
                            if nametable == Nametable::Fill =>
                        {
                            if offset < 0x03C0 {
                                self.regs.fill_tile
                            } else {
                                ATTR_BITS[(self.regs.fill_attr as usize) & 0x03]
                            }
                        }
                        _ => 0,
                    }
                }
            }
            0x5010 => {
                // [I... ...M] DMC
                //   I = IRQ (0 = No IRQ triggered. 1 = IRQ was triggered.) Reading $5010 acknowledges the IRQ and clears this flag.
                //   M = Mode select (0 = write mode. 1 = read mode.)
                let irq = self.dmc.irq_pending && self.dmc.irq_enabled;
                (irq as u8) << 7 | self.dmc_mode
            }
            0x5100 => self.regs.prg_mode as u8,
            0x5101 => self.regs.chr_mode as u8,
            0x5104 => self.regs.exram_mode as u8,
            0x5105 => self.regs.nametable_mirroring,
            0x5106 => self.regs.fill_tile,
            0x5107 => self.regs.fill_attr,
            0x5015 => {
                // [.... ..BA]   Length status for Pulse 1 (A), 2 (B)
                let mut status = 0b00;
                if self.pulse1.length.counter > 0 {
                    status |= 0x01;
                }
                if self.pulse2.length.counter > 0 {
                    status |= 0x02;
                }
                status
            }
            0x5113..=0x5117 => {
                let bank = (addr - 0x5113) as usize;
                self.regs.prg_banks[bank] as u8
            }
            0x5120..=0x512B => {
                let bank = (addr - 0x5120) as usize;
                self.regs.chr_banks[bank] as u8
            }
            0x5130 => self.regs.chr_hi as u8,
            0x5200 => {
                (self.regs.vsplit_enabled as u8) << 7
                    | (self.regs.vsplit_side as u8) << 6
                    | self.regs.vsplit_tile
            }
            0x5201 => self.regs.vsplit_scroll,
            0x5202 => self.regs.vsplit_bank,
            0x5203 => self.regs.scanline_num_irq as u8,
            0x5204 => {
                // $5204:  [PI.. ....]
                //   P = IRQ currently pending
                //   I = "In Frame" signal

                // Reading $5204 will clear the pending flag (acknowledging the IRQ).
                // Clearing is done in the read() function
                (self.irq_pending as u8) << 7 | (self.regs.in_frame as u8) << 6
            }
            0x5205 => (self.regs.mult_result & 0xFF) as u8,
            0x5206 => ((self.regs.mult_result >> 8) & 0xFF) as u8,
            0x5207 => self.open_bus, // TODO MMC5A only CL3 / SL3 Data Direction and Output Data Source
            0x5208 => self.open_bus, // TODO MMC5A only CL3 / SL3 Status
            0x5209 => self.open_bus, // TODO MMC5A only 6-bit Hardware Timer with IRQ
            0x5800..=0x5BFF => self.open_bus, // MMC5A unknown - reads open_bus
            0x5C00..=0x5FFF => {
                match self.regs.exram_mode {
                    // nametable/attr modes are not used for RAM, thus are not readable
                    ExRamMode::Nametable | ExRamMode::ExAttr => self.open_bus,
                    _ => self.exram.peek(addr - 0x5C00),
                }
            }
            0x6000..=0xDFFF => {
                if self.rom_select(addr) {
                    self.prg_rom.peek(addr)
                } else {
                    self.prg_ram.peek(addr)
                }
            }
            0xE000..=0xFFFF => self.prg_rom.peek(addr),
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Exrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x2000..=0x3EFF => {
                let nametable = self.nametable_mapping(addr);
                match self.regs.exram_mode {
                    ExRamMode::Nametable | ExRamMode::ExAttr if nametable == Nametable::ExRAM => {
                        self.exram.write(addr - 0x2000, val);
                    }
                    _ => (),
                }
            }
            0x5000 => self.pulse1.write_control(val),
            // 0x5001 Has no effect since there is no Sweep unit
            0x5002 => self.pulse1.write_timer_lo(val),
            0x5003 => self.pulse1.write_timer_hi(val),
            0x5004 => self.pulse2.write_control(val),
            // 0x5005 Has no effect since there is no Sweep unit
            0x5006 => self.pulse2.write_timer_lo(val),
            0x5007 => self.pulse2.write_timer_hi(val),
            0x5010 => {
                // [I... ...M] DMC
                //   I = PCM IRQ enable (1 = enabled.)
                //   M = Mode select (0 = write mode. 1 = read mode.)
                self.dmc_mode = val & 0x01;
                self.dmc.irq_enabled = val & 0x80 > 0;
            }
            0x5011 => {
                // [DDDD DDDD] PCM Data
                if self.dmc_mode == 0 {
                    // Write mode
                    self.dmc.output = val;
                }
            }
            0x5015 => {
                //  [.... ..BA]   Enable flags for Pulse 1 (A), 2 (B)  (0=disable, 1=enable)
                self.pulse1.enabled = val & 1 == 1;
                if !self.pulse1.enabled {
                    self.pulse1.length.counter = 0;
                }
                self.pulse2.enabled = (val >> 1) & 1 == 1;
                if !self.pulse2.enabled {
                    self.pulse2.length.counter = 0;
                }
            }
            0x5100 => {
                // [.... ..PP] PRG Mode
                self.regs.prg_mode = PrgMode::from(val);
                self.update_prg_banks();
            }
            0x5101 => {
                // [.... ..CC] CHR Mode
                self.regs.chr_mode = ChrMode::from(val);
                self.update_chr_banks(self.regs.last_chr_write);
            }
            0x5102 | 0x5103 => {
                // [.... ..AA]    PRG-RAM Protect A
                // [.... ..BB]    PRG-RAM Protect B
                self.regs.prg_ram_protect[(addr - 0x5102) as usize] = val & 0x03;
                self.update_ram_protection();
            }
            0x5104 => self.regs.exram_mode = ExRamMode::from(val), // [.... ..XX] ExRAM mode
            0x5105 => {
                // [.... ..HH]
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
                self.regs.nametable_mirroring = val;
                self.mirroring = Mirroring::from(self.regs.nametable_mirroring);
            }
            0x5106 => self.regs.fill_tile = val, // [TTTT TTTT] Fill Tile
            0x5107 => self.regs.fill_attr = val & 0x03, // [.... ..AA] Fill Attribute bits
            0x5113..=0x5117 => {
                // PRG Bank Switching
                // $5113: [.... .PPP]
                //      8k PRG-RAM @ $6000
                // $5114-5117: [RPPP PPPP]
                //      R = ROM select (0=select RAM, 1=select ROM)  **unused in $5117**
                //      P = PRG page
                let bank = (addr - 0x5113) as usize;
                self.regs.prg_banks[bank] = val as usize;
                self.update_prg_banks();
            }
            0x5120..=0x512B => {
                let bank = (addr - 0x5120) as usize;
                self.regs.chr_banks[bank] = (self.regs.chr_hi << 8) | val as usize;
                if addr < 0x5128 {
                    self.update_chr_banks(ChrBank::Spr);
                } else {
                    // Mirroring BG
                    self.regs.chr_banks[bank + 4] = self.regs.chr_banks[bank];
                    self.update_chr_banks(ChrBank::Bg);
                }
            }
            0x5130 => self.regs.chr_hi = (val & 0x03) as usize, // [.... ..HH]  CHR Bank Hi bits
            0x5200 => {
                // [ES.T TTTT]    Split control
                //   E = Enable  (0=split mode disabled, 1=split mode enabled)
                //   S = Vsplit side  (0=split will be on left side, 1=split will be on right)
                //   T = tile number to split at
                self.regs.vsplit_enabled = val & 0x80 == 0x80;
                self.regs.vsplit_side = if val & 0x40 == 0x40 {
                    Split::Right
                } else {
                    Split::Left
                };
                self.regs.vsplit_tile = val & 0x1F;
            }
            0x5201 => self.regs.vsplit_scroll = val, // [YYYY YYYY]  Split Y scroll
            0x5202 => self.regs.vsplit_bank = val,   // [CCCC CCCC]  4k CHR Page for split
            0x5203 => self.regs.scanline_num_irq = u16::from(val), // [IIII IIII]  IRQ Target
            0x5204 => {
                // [E... ....]    IRQ Enable (0=disabled, 1=enabled)
                self.regs.irq_enabled = val & 0x80 > 0;
            }
            0x5205 => {
                self.regs.multiplicand = val;
                self.regs.mult_result =
                    u16::from(self.regs.multiplicand) * u16::from(self.regs.multiplier);
            }
            0x5206 => {
                self.regs.multiplier = val;
                self.regs.mult_result =
                    u16::from(self.regs.multiplicand) * u16::from(self.regs.multiplier);
            }
            0x5207 => (), // TODO MMC5A only CL3 / SL3 Data Direction and Output Data Source
            0x5208 => (), // TODO MMC5A only CL3 / SL3 Status
            0x5209 => (), // TODO MMC5A only 6-bit Hardware Timer with IRQ
            0x5800..=0x5BFF => (), // MMC5A unknown
            0x5C00..=0x5FFF => {
                let addr = addr - 0x5C00;
                match self.regs.exram_mode {
                    ExRamMode::Nametable | ExRamMode::ExAttr => {
                        if self.ppu_rendering {
                            self.exram.write(addr, val);
                        } else {
                            self.exram.write(addr, 0x00);
                        }
                    }
                    ExRamMode::Ram => self.exram.write(addr, val),
                    _ => (), // Not writable
                }
            }
            0x6000..=0xDFFF => {
                if !self.rom_select(addr) {
                    self.prg_ram.write(addr, val)
                }
            }
            // 0x0000..=0x1FFF CHR-ROM is read-only
            // 0xE000..=0xFFFF ROM is write-only
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
        self.regs.prg_mode = PrgMode::Bank8k;
        self.regs.chr_mode = ChrMode::Bank1k;
    }
}

impl Savable for Exrom {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.regs.save(fh)?;
        self.mirroring.save(fh)?;
        self.irq_pending.save(fh)?;
        self.spr_fetch_count.save(fh)?;
        self.ppu_prev_addr.save(fh)?;
        self.ppu_prev_match.save(fh)?;
        self.ppu_reading.save(fh)?;
        self.ppu_idle.save(fh)?;
        self.ppu_in_vblank.save(fh)?;
        self.ppu_rendering.save(fh)?;
        self.prg_ram.save(fh)?;
        self.exram.save(fh)?;
        self.tile_cache.save(fh)?;
        self.in_split.save(fh)?;
        self.split_tile.save(fh)?;
        self.pulse1.save(fh)?;
        self.pulse2.save(fh)?;
        self.dmc.save(fh)?;
        self.dmc_mode.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.regs.load(fh)?;
        self.mirroring.load(fh)?;
        self.irq_pending.load(fh)?;
        self.spr_fetch_count.load(fh)?;
        self.ppu_prev_addr.load(fh)?;
        self.ppu_prev_match.load(fh)?;
        self.ppu_reading.load(fh)?;
        self.ppu_idle.load(fh)?;
        self.ppu_in_vblank.load(fh)?;
        self.ppu_rendering.load(fh)?;
        self.prg_ram.load(fh)?;
        self.exram.load(fh)?;
        self.tile_cache.load(fh)?;
        self.in_split.load(fh)?;
        self.split_tile.load(fh)?;
        self.pulse1.load(fh)?;
        self.pulse2.load(fh)?;
        self.dmc.load(fh)?;
        self.dmc_mode.load(fh)?;
        Ok(())
    }
}

impl Savable for ExRegs {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.sprite8x16.save(fh)?;
        self.prg_mode.save(fh)?;
        self.chr_mode.save(fh)?;
        self.chr_hi.save(fh)?;
        self.prg_ram_protect.save(fh)?;
        self.exram_mode.save(fh)?;
        self.nametable_mirroring.save(fh)?;
        self.fill_tile.save(fh)?;
        self.fill_attr.save(fh)?;
        self.prg_banks.save(fh)?;
        self.chr_banks.save(fh)?;
        self.last_chr_write.save(fh)?;
        self.vsplit_enabled.save(fh)?;
        self.vsplit_side.save(fh)?;
        self.vsplit_tile.save(fh)?;
        self.vsplit_scroll.save(fh)?;
        self.vsplit_bank.save(fh)?;
        self.scanline_num_irq.save(fh)?;
        self.irq_enabled.save(fh)?;
        self.irq_counter.save(fh)?;
        self.in_frame.save(fh)?;
        self.multiplicand.save(fh)?;
        self.multiplier.save(fh)?;
        self.mult_result.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.sprite8x16.load(fh)?;
        self.prg_mode.load(fh)?;
        self.chr_mode.load(fh)?;
        self.chr_hi.load(fh)?;
        self.prg_ram_protect.load(fh)?;
        self.exram_mode.load(fh)?;
        self.nametable_mirroring.load(fh)?;
        self.fill_tile.load(fh)?;
        self.fill_attr.load(fh)?;
        self.prg_banks.load(fh)?;
        self.chr_banks.load(fh)?;
        self.last_chr_write.load(fh)?;
        self.vsplit_enabled.load(fh)?;
        self.vsplit_side.load(fh)?;
        self.vsplit_tile.load(fh)?;
        self.vsplit_scroll.load(fh)?;
        self.vsplit_bank.load(fh)?;
        self.scanline_num_irq.load(fh)?;
        self.irq_enabled.load(fh)?;
        self.irq_counter.load(fh)?;
        self.in_frame.load(fh)?;
        self.multiplicand.load(fh)?;
        self.multiplier.load(fh)?;
        self.mult_result.load(fh)?;
        Ok(())
    }
}

impl Savable for PrgMode {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = PrgMode::from(val);
        Ok(())
    }
}

impl From<u8> for PrgMode {
    fn from(val: u8) -> Self {
        match val & 0x03 {
            0 => PrgMode::Bank32k,
            1 => PrgMode::Bank16k,
            2 => PrgMode::Bank16_8k,
            3 => PrgMode::Bank8k,
            _ => unreachable!("invalid PrgMode"),
        }
    }
}

impl Savable for ChrBank {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
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

impl Savable for ChrMode {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = ChrMode::from(val);
        Ok(())
    }
}

impl From<u8> for ChrMode {
    fn from(val: u8) -> Self {
        match val & 0x03 {
            0 => ChrMode::Bank8k,
            1 => ChrMode::Bank4k,
            2 => ChrMode::Bank2k,
            3 => ChrMode::Bank1k,
            _ => unreachable!("invalid ChrMode"),
        }
    }
}

impl Savable for ExRamMode {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = ExRamMode::from(val);
        Ok(())
    }
}

impl From<u8> for ExRamMode {
    fn from(val: u8) -> Self {
        match val {
            0 => ExRamMode::Nametable,
            1 => ExRamMode::ExAttr,
            2 => ExRamMode::Ram,
            3 => ExRamMode::RamProtected,
            _ => panic!("invalid ExRamMode {}", val),
        }
    }
}

impl Savable for Split {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => Split::Left,
            1 => Split::Right,
            _ => panic!("invalid Split value"),
        };
        Ok(())
    }
}

impl From<u8> for Mirroring {
    fn from(val: u8) -> Self {
        use Mirroring::*;
        match val {
            0x50 => Horizontal,
            0x44 => Vertical,
            0x00 => SingleScreenA,
            0x55 => SingleScreenB,
            // While the below technically isn't true - it forces my implementation to
            // rely on the Mapper for reading Nametables in any other mode for the missing
            // two nametables
            _ => FourScreen,
        }
    }
}

impl From<Mirroring> for u8 {
    fn from(mirroring: Mirroring) -> Self {
        use Mirroring::*;
        match mirroring {
            Horizontal => 0x50,
            Vertical => 0x44,
            SingleScreenA => 0x00,
            SingleScreenB => 0x55,
            FourScreen => 0xFF,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn prg_ram_protect() {
        use super::*;
        use crate::{cartridge::Cartridge, memory::Memory};
        let consistent_ram = true;
        for a in 0..4 {
            for b in 0..4 {
                let mut cart = Cartridge::new();
                cart.prg_rom = Memory::rom(0xFFFF);
                let mut exrom = Exrom::load(cart, consistent_ram);

                exrom.write(0x5102, a);
                exrom.write(0x5103, b);
                exrom.write(0x5114, 0);
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
