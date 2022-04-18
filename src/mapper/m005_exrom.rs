//! `ExROM`/`MMC5` (Mapper 5)
//!
//! <https://wiki.nesdev.com/w/index.php/ExROM>
//! <https://wiki.nesdev.com/w/index.php/MMC5>

use crate::{
    apu::{
        dmc::Dmc,
        pulse::{OutputFreq, Pulse, PulseChannel},
    },
    cart::Cart,
    common::{Clocked, Powered},
    mapper::{MapRead, MapWrite, Mapped, MappedRead, MappedWrite, Mapper, MirroringType},
    memory::{MemRead, MemWrite, Memory, MemoryBanks},
    ppu::{
        vram::{ATTR_OFFSET, NT_SIZE, NT_START},
        Mirroring,
    },
};
use serde::{Deserialize, Serialize};

const PRG_WINDOW: usize = 8 * 1024;
const PRG_RAM_SIZE: usize = 64 * 1024; // Provide 64K since mappers don't always specify
const EXRAM_SIZE: usize = 1024;
const CHR_ROM_WINDOW: usize = 1024;

const ROM_SELECT_MASK: usize = 0x80; // High bit targets ROM bank switching
const BANK_MASK: usize = 0x7F; // Ignore high bit for ROM select

const START_SPR_FETCH: u32 = 64;
const END_SPR_FETCH: u32 = 81;

const ATTR_BITS: [u8; 4] = [0x00, 0x55, 0xAA, 0xFF];
// TODO: See about generating these using oncecell
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum PrgMode {
    Bank32k,
    Bank16k,
    Bank16_8k,
    Bank8k,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum ChrMode {
    Bank8k,
    Bank4k,
    Bank2k,
    Bank1k,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum ExMode {
    Nametable,
    Attr,
    Ram,
    RamProtected,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum ChrBank {
    Spr,
    Bg,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Nametable {
    ScreenA,
    ScreenB,
    ExRAM,
    Fill,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Fill {
    pub tile: u8, // $5106
    pub attr: u8, // $5107
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum SplitSide {
    Left,
    Right,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct VSplit {
    pub enabled: bool,   // $5200 [E... ....]
    pub side: SplitSide, // $5200 [.S.. ....]
    pub tile: u8,        // $5200 [...T TTTT]
    pub scroll: u8,      // $5201
    pub bank: u8,        // $5202
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct ExRegs {
    pub prg_mode: PrgMode,        // $5100
    pub chr_mode: ChrMode,        // $5101
    pub prg_ram_protect: [u8; 2], // $5102 - $5103
    pub exmode: ExMode,           // $5104
    pub nametable_mirroring: u8,  // $5105
    pub fill: Fill,               // $5106 - $5107
    pub prg_banks: [usize; 5],    // $5113 - $5117
    pub chr_banks: [usize; 16],   // $5120 - $512B
    pub chr_hi: usize,            // $5130
    pub vsplit: VSplit,           // $5200 - $5202
    pub irq_scanline: u16,        // $5203: Write $00 to disable IRQs
    pub irq_enabled: bool,        // $5204
    pub multiplicand: u8,         // $5205: write
    pub multiplier: u8,           // $5206: write
    pub mult_result: u16,         // $5205: read lo, $5206: read hi
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct PpuStatus {
    pub fetch_count: u32,
    pub prev_addr: u16,
    pub prev_match: u8,
    pub reading: bool,
    pub idle: u8,
    pub in_vblank: bool,
    pub sprite8x16: bool, // $2000 PPUCTRL: false = 8x8, true = 8x16
    pub rendering: bool,
    pub scanline: u16,
    pub in_frame: bool,
}

impl PpuStatus {
    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x2000 => self.sprite8x16 = val & 0x20 > 0,
            0x2001 => {
                self.rendering = val & 0x18 > 0; // 1, 2, or 3
                if !self.rendering {
                    self.in_frame = false;
                    self.prev_addr = 0x0000;
                }
            }
            0x2002 => self.in_vblank = val & 0x80 > 0,
            _ => (),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Exrom {
    pub regs: ExRegs,
    pub mirroring: Mirroring,
    pub irq_pending: bool,
    pub ppu_status: PpuStatus,
    pub exram: Memory,
    pub prg_ram_banks: MemoryBanks,
    pub prg_rom_banks: MemoryBanks,
    pub chr_banks: MemoryBanks,
    pub tile_cache: u16,
    pub in_split: bool,
    pub split_tile: u16,
    pub last_chr_write: ChrBank,
    pub pulse1: Pulse,
    pub pulse2: Pulse,
    pub dmc: Dmc,
    pub dmc_mode: u8,
}

impl ExRegs {
    const fn new(mirroring: Mirroring) -> Self {
        Self {
            prg_mode: PrgMode::Bank8k,
            chr_mode: ChrMode::Bank1k,
            prg_ram_protect: [0x00; 2],
            exmode: ExMode::RamProtected,
            nametable_mirroring: match mirroring {
                Mirroring::Horizontal => 0x50,
                Mirroring::Vertical => 0x44,
                Mirroring::SingleScreenA => 0x00,
                Mirroring::SingleScreenB => 0x55,
                Mirroring::FourScreen => 0xFF,
            },
            fill: Fill {
                tile: 0xFF,
                attr: 0xFF,
            },
            prg_banks: [0x00; 5],
            chr_banks: [0x00; 16],
            chr_hi: 0x00,
            vsplit: VSplit {
                enabled: false,
                side: SplitSide::Left,
                tile: 0x00,
                scroll: 0x00,
                bank: 0x00,
            },
            irq_scanline: 0x00,
            irq_enabled: false,
            multiplicand: 0xFF,
            multiplier: 0xFF,
            mult_result: 0xFE01, // e.g. 0xFF * 0xFF
        }
    }
}

impl Exrom {
    pub fn load(cart: &mut Cart) -> Mapper {
        cart.prg_ram.resize(PRG_RAM_SIZE);

        let mirroring = cart.mirroring();
        let mut exrom = Self {
            regs: ExRegs::new(mirroring),
            mirroring,
            irq_pending: false,
            ppu_status: PpuStatus {
                fetch_count: 0x00,
                prev_addr: 0xFFFF,
                prev_match: 0x0000,
                reading: false,
                idle: 0x00,
                in_vblank: false,
                sprite8x16: false,
                rendering: false,
                scanline: 0x0000,
                in_frame: false,
            },
            exram: Memory::ram(EXRAM_SIZE, cart.ram_state),
            prg_ram_banks: MemoryBanks::new(0x6000, 0xFFFF, cart.prg_ram.len(), PRG_WINDOW),
            prg_rom_banks: MemoryBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), PRG_WINDOW),
            chr_banks: MemoryBanks::new(0x0000, 0x1FFF, cart.chr.len(), CHR_ROM_WINDOW),
            tile_cache: 0x0000,
            in_split: false,
            split_tile: 0x0000,
            last_chr_write: ChrBank::Spr,
            pulse1: Pulse::new(PulseChannel::One, OutputFreq::Ultrasonic),
            pulse2: Pulse::new(PulseChannel::Two, OutputFreq::Ultrasonic),
            dmc: Dmc::new(),
            dmc_mode: 0x01, // Default to read mode
        };
        exrom.regs.prg_banks[4] = exrom.prg_rom_banks.last() | ROM_SELECT_MASK;
        exrom.update_prg_banks();
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
        let mode = self.regs.prg_mode;
        let banks = self.regs.prg_banks;

        self.prg_ram_banks.set(0, banks[0]); // $5113 always selects RAM
        match mode {
            // $5117 always selects ROM
            PrgMode::Bank32k => self.prg_rom_banks.set_range(0, 3, banks[4]),
            PrgMode::Bank16k => {
                self.set_prg_bank_range(0, 1, banks[2]);
                self.prg_rom_banks.set_range(2, 3, banks[4] & BANK_MASK);
            }
            PrgMode::Bank16_8k => {
                self.set_prg_bank_range(0, 1, banks[2]);
                self.set_prg_bank_range(2, 2, banks[3]);
                self.prg_rom_banks.set(3, banks[4] & BANK_MASK);
            }
            PrgMode::Bank8k => {
                self.set_prg_bank_range(0, 0, banks[1]);
                self.set_prg_bank_range(1, 1, banks[2]);
                self.set_prg_bank_range(2, 2, banks[3]);
                self.prg_rom_banks.set(3, banks[4] & BANK_MASK);
            }
        };
    }

    #[inline]
    fn set_prg_bank_range(&mut self, start: usize, end: usize, bank: usize) {
        let rom = bank & ROM_SELECT_MASK == ROM_SELECT_MASK;
        let bank = bank & BANK_MASK;
        if rom {
            self.prg_rom_banks.set_range(start, end, bank);
        } else {
            self.prg_ram_banks.set_range(start + 1, end + 1, bank);
        }
    }

    #[inline]
    fn rom_select(&self, addr: u16) -> bool {
        let mode = self.regs.prg_mode;
        if matches!(addr, 0x6000..=0x7FFF) {
            false
        } else if matches!(addr, 0xE000..=0xFFFF) || mode == PrgMode::Bank32k {
            true
        } else {
            use PrgMode::{Bank16_8k, Bank16k, Bank8k};
            let banks = self.regs.prg_banks;
            let bank = match (addr, mode) {
                (0x8000..=0x9FFF, Bank8k) => banks[1],
                (0x8000..=0xBFFF, Bank16k | Bank16_8k) | (0xA000..=0xBFFF, Bank8k) => banks[2],
                (0xC000..=0xDFFF, Bank8k | Bank16_8k) => banks[3],
                (0xC000..=0xDFFF, Bank16k) => banks[4],
                _ => 0x00,
            };
            bank & ROM_SELECT_MASK == ROM_SELECT_MASK
        }
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
    fn update_chr_banks(&mut self, chr_bank: ChrBank) {
        let hi = self.regs.chr_hi;
        let banks = match chr_bank {
            ChrBank::Spr => &self.regs.chr_banks[0..8],
            ChrBank::Bg => &self.regs.chr_banks[8..16],
        };
        // CHR banks are in actual page sizes which means they need to be shifted appropriately
        match self.regs.chr_mode {
            ChrMode::Bank8k => self.chr_banks.set_range(0, 7, hi | banks[7] << 3),
            ChrMode::Bank4k => {
                self.chr_banks.set_range(0, 3, hi | banks[3] << 2);
                self.chr_banks.set_range(4, 7, hi | banks[7] << 2);
            }
            ChrMode::Bank2k => {
                self.chr_banks.set_range(0, 1, hi | banks[1] << 1);
                self.chr_banks.set_range(2, 3, hi | banks[3] << 1);
                self.chr_banks.set_range(4, 5, hi | banks[5] << 1);
                self.chr_banks.set_range(6, 7, hi | banks[7] << 1);
            }
            ChrMode::Bank1k => {
                self.chr_banks.set(0, hi | banks[0]);
                self.chr_banks.set(1, hi | banks[1]);
                self.chr_banks.set(2, hi | banks[2]);
                self.chr_banks.set(3, hi | banks[3]);
                self.chr_banks.set(4, hi | banks[4]);
                self.chr_banks.set(5, hi | banks[5]);
                self.chr_banks.set(6, hi | banks[6]);
                self.chr_banks.set(7, hi | banks[7]);
            }
        };
    }

    // Determine the nametable we're trying to access
    #[inline]
    fn nametable_mapping(&self, addr: u16) -> Nametable {
        let addr = (addr - NT_START) % (4 * NT_SIZE);
        let table = addr / NT_SIZE;
        match (self.regs.nametable_mirroring >> (2 * table)) & 0x03 {
            0 => Nametable::ScreenA,
            1 => Nametable::ScreenB,
            2 => Nametable::ExRAM,
            3 => Nametable::Fill,
            _ => unreachable!("invalid mirroring"),
        }
    }
}

impl Mapped for Exrom {
    #[inline]
    fn irq_pending(&self) -> bool {
        self.regs.irq_enabled && self.irq_pending
    }

    #[inline]
    fn mirroring(&self) -> MirroringType {
        self.mirroring.into()
    }

    // Used by the PPU to determine whether it should use it's own internal CIRAM for nametable
    // reads or to read CIRAM instead from the mapper
    #[inline]
    fn use_ciram(&self, addr: u16) -> bool {
        if self.in_split
            || (self.regs.exmode == ExMode::Attr
                && (addr & 0x03FF) >= ATTR_OFFSET
                && (self.ppu_status.fetch_count < START_SPR_FETCH
                    || self.ppu_status.fetch_count >= END_SPR_FETCH))
        {
            // If we're in Extended Attribute mode and reading BG attributes,
            // yield to mapper for Attribute data instead of PPU
            false
        } else {
            // 0 and 1 mean NametableA and NametableB
            // 2 means internal EXRAM
            // 3 means Fill-mode
            let nametable = self.nametable_mapping(addr);
            matches!(nametable, Nametable::ScreenA | Nametable::ScreenB)
        }
    }

    // Returns a nametable page based on $5105 nametable mapping
    // 0/1 use PPU CIRAM, 2/3 use EXRAM/Fill-mode
    #[inline]
    fn nametable_page(&self, addr: u16) -> u16 {
        let nametable = self.nametable_mapping(addr);
        match nametable {
            Nametable::ScreenA | Nametable::ScreenB => nametable as u16,
            _ => 0,
        }
    }

    fn ppu_read(&mut self, addr: u16) {
        // Ignore palette reads
        if addr > 0x3EFF {
            return;
        }

        if matches!(addr, 0x2000..=0x3EFF)
            && self.regs.exmode == ExMode::Attr
            && (addr & 0x03FF) < ATTR_OFFSET
            && (self.ppu_status.fetch_count < START_SPR_FETCH
                || self.ppu_status.fetch_count >= END_SPR_FETCH)
        {
            self.tile_cache = addr & 0x03FF;
        }

        // https://wiki.nesdev.org/w/index.php?title=MMC5#Scanline_Detection_and_Scanline_IRQ
        let status = &mut self.ppu_status;
        if matches!(addr, 0x2000..=0x2FFF) && addr == status.prev_addr {
            status.prev_match += 1;
            if status.prev_match == 2 {
                if status.in_frame {
                    status.scanline = status.scanline.wrapping_add(1);
                    if status.scanline == self.regs.irq_scanline {
                        self.irq_pending = true;
                    }
                } else {
                    status.in_frame = true;
                    status.scanline = 0;
                }
                status.fetch_count = 0;
            }
        } else {
            status.prev_match = 0;
        }
        status.prev_addr = addr;
        status.reading = true;
    }

    #[inline]
    fn ppu_write(&mut self, addr: u16, val: u8) {
        self.ppu_status.write(addr, val);
    }
}

impl MapRead for Exrom {
    fn map_read(&mut self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => {
                self.ppu_status.fetch_count += 1;

                if self.ppu_status.sprite8x16 {
                    match self.ppu_status.fetch_count {
                        START_SPR_FETCH => self.update_chr_banks(ChrBank::Spr),
                        END_SPR_FETCH => self.update_chr_banks(ChrBank::Bg),
                        _ => (),
                    }
                }
            }
            0x2000..=0x3EFF => {
                // Detect split
                let offset = addr % NT_SIZE;
                if self.in_split && offset < ATTR_OFFSET {
                    self.split_tile = (u16::from(self.regs.vsplit.scroll & 0xF8) << 2)
                        | ((self.ppu_status.fetch_count / 4) & 0x1F) as u16;
                }
            }
            0x5204 => self.irq_pending = false, // Reading from IRQ status clears it
            0x5010 => self.dmc.irq_pending = false,
            0xFFFA | 0xFFFB => {
                self.ppu_status.in_frame = false; // NMI clears in_frame
                self.ppu_status.prev_addr = 0x0000;
            }
            _ => (),
        }
        self.map_peek(addr)
    }

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => {
                if self.regs.exmode == ExMode::Attr
                    && (self.ppu_status.fetch_count < START_SPR_FETCH
                        || self.ppu_status.fetch_count >= END_SPR_FETCH)
                {
                    let hibits = self.regs.chr_hi << 10;
                    let exbits = (self.exram.peek(self.tile_cache) as usize & 0x3F) << 12;
                    let addr = hibits | exbits | (addr as usize) & 0x0FFF;
                    MappedRead::Chr(addr)
                } else {
                    MappedRead::Chr(self.chr_banks.translate(addr))
                }
            }
            0x2000..=0x3EFF => {
                let offset = addr % NT_SIZE;
                if self.in_split {
                    if offset < ATTR_OFFSET {
                        MappedRead::Data(self.exram.peek(self.split_tile))
                    } else {
                        let addr =
                            ATTR_OFFSET | u16::from(ATTR_LOC[(self.split_tile as usize) >> 2]);
                        let attr = self.exram.peek(addr - 0x2000) as usize;
                        let shift = ATTR_SHIFT[(self.split_tile as usize) & 0x7F] as usize;
                        MappedRead::Data(ATTR_BITS[(attr >> shift) & 0x03])
                    }
                } else {
                    match self.regs.exmode {
                        ExMode::Attr if offset >= ATTR_OFFSET => {
                            let attr = self.exram.peek(self.tile_cache) as usize;
                            MappedRead::Data(ATTR_BITS[(attr >> 6) & 0x03])
                        }
                        ExMode::Nametable | ExMode::Attr => match self.nametable_mapping(addr) {
                            Nametable::ExRAM => MappedRead::Data(self.exram.peek(addr - 0x2000)),
                            Nametable::Fill => {
                                if offset < ATTR_OFFSET {
                                    MappedRead::Data(self.regs.fill.tile)
                                } else {
                                    MappedRead::Data(
                                        ATTR_BITS[(self.regs.fill.attr as usize) & 0x03],
                                    )
                                }
                            }
                            _ => MappedRead::None,
                        },
                        _ => MappedRead::None,
                    }
                }
            }
            0x5010 => {
                // [I... ...M] DMC
                //   I = IRQ (0 = No IRQ triggered. 1 = IRQ was triggered.) Reading $5010 acknowledges the IRQ and clears this flag.
                //   M = Mode select (0 = write mode. 1 = read mode.)
                let irq = self.dmc.irq_pending && self.dmc.irq_enabled;
                MappedRead::Data(u8::from(irq) << 7 | self.dmc_mode)
            }
            0x5100 => MappedRead::Data(self.regs.prg_mode as u8),
            0x5101 => MappedRead::Data(self.regs.chr_mode as u8),
            0x5104 => MappedRead::Data(self.regs.exmode as u8),
            0x5105 => MappedRead::Data(self.regs.nametable_mirroring),
            0x5106 => MappedRead::Data(self.regs.fill.tile),
            0x5107 => MappedRead::Data(self.regs.fill.attr),
            0x5015 => {
                // [.... ..BA]   Length status for Pulse 1 (A), 2 (B)
                let mut status = 0b00;
                if self.pulse1.length.counter > 0 {
                    status |= 0x01;
                }
                if self.pulse2.length.counter > 0 {
                    status |= 0x02;
                }
                MappedRead::Data(status)
            }
            0x5113..=0x5117 => {
                let bank = (addr - 0x5113) as usize;
                MappedRead::Data(self.regs.prg_banks[bank] as u8)
            }
            0x5120..=0x512B => {
                let bank = (addr - 0x5120) as usize;
                MappedRead::Data(self.regs.chr_banks[bank] as u8)
            }
            0x5130 => MappedRead::Data(self.regs.chr_hi as u8),
            0x5200 => MappedRead::Data(
                u8::from(self.regs.vsplit.enabled) << 7
                    | (self.regs.vsplit.side as u8) << 6
                    | self.regs.vsplit.tile,
            ),
            0x5201 => MappedRead::Data(self.regs.vsplit.scroll),
            0x5202 => MappedRead::Data(self.regs.vsplit.bank),
            0x5203 => MappedRead::Data(self.regs.irq_scanline as u8),
            0x5204 => {
                // $5204:  [PI.. ....]
                //   P = IRQ currently pending
                //   I = "In Frame" signal

                // Reading $5204 will clear the pending flag (acknowledging the IRQ).
                // Clearing is done in the read() function
                MappedRead::Data(
                    u8::from(self.irq_pending) << 7 | u8::from(self.ppu_status.in_frame) << 6,
                )
            }
            0x5205 => MappedRead::Data((self.regs.mult_result & 0xFF) as u8),
            0x5206 => MappedRead::Data(((self.regs.mult_result >> 8) & 0xFF) as u8),
            0x5C00..=0x5FFF if !matches!(self.regs.exmode, ExMode::Nametable | ExMode::Attr) => {
                // Nametable/Attr modes are not used for RAM, thus are not readable
                MappedRead::Data(self.exram.peek(addr - 0x5C00))
            }
            0x6000..=0xDFFF => {
                if self.rom_select(addr) {
                    MappedRead::PrgRom(self.prg_rom_banks.translate(addr))
                } else {
                    MappedRead::PrgRam(self.prg_ram_banks.translate(addr))
                }
            }
            0xE000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            // TODO MMC5A only CL3 / SL3 Data Direction and Output Data Source
            // TODO MMC5A only CL3 / SL3 Status
            // TODO MMC5A only 6-bit Hardware Timer with IRQ
            0x5207 | 0x5208 | 0x5209 => MappedRead::Data(0),
            // 0x5800..=0x5BFF - MMC5A unknown - reads open_bus
            _ => MappedRead::None,
        }
    }
}

impl MapWrite for Exrom {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x2000..=0x3EFF => {
                let nametable = self.nametable_mapping(addr);
                match self.regs.exmode {
                    ExMode::Nametable | ExMode::Attr if nametable == Nametable::ExRAM => {
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
                    if val == 0x00 {
                        self.dmc.irq_enabled = true;
                    } else {
                        self.dmc.output = val;
                    }
                }
            }
            0x5015 => {
                //  [.... ..BA]   Enable flags for Pulse 1 (A), 2 (B)  (0=disable, 1=enable)
                self.pulse1.set_enabled(val & 0x01 == 0x01);
                self.pulse2.set_enabled(val & 0x10 == 0x10);
            }
            0x5100 => {
                // [.... ..PP] PRG Mode
                self.regs.prg_mode = match val & 0x03 {
                    0 => PrgMode::Bank32k,
                    1 => PrgMode::Bank16k,
                    2 => PrgMode::Bank16_8k,
                    3 => PrgMode::Bank8k,
                    _ => {
                        log::warn!("invalid PrgMode value: ${:02X}", val);
                        self.regs.prg_mode
                    }
                };
                self.update_prg_banks();
            }
            0x5101 => {
                // [.... ..CC] CHR Mode
                self.regs.chr_mode = match val & 0x03 {
                    0 => ChrMode::Bank8k,
                    1 => ChrMode::Bank4k,
                    2 => ChrMode::Bank2k,
                    3 => ChrMode::Bank1k,
                    _ => {
                        log::warn!("invalid ChrMode value: ${:02X}", val);
                        self.regs.chr_mode
                    }
                };
                self.update_chr_banks(self.last_chr_write);
            }
            0x5102 | 0x5103 => {
                // [.... ..AA]    PRG-RAM Protect A
                // [.... ..BB]    PRG-RAM Protect B
                self.regs.prg_ram_protect[(addr - 0x5102) as usize] = val & 0x03;
                // To allow writing to PRG-RAM you must set:
                //    A=%10
                //    B=%01
                // Any other value will prevent PRG-RAM writing.
                let writable =
                    self.regs.prg_ram_protect[0] == 0b10 && self.regs.prg_ram_protect[1] == 0b01;
                return MappedWrite::PrgRamProtect(!writable);
            }
            0x5104 => {
                // [.... ..XX] ExRAM mode
                self.regs.exmode = match val {
                    0 => ExMode::Nametable,
                    1 => ExMode::Attr,
                    2 => ExMode::Ram,
                    3 => ExMode::RamProtected,
                    _ => {
                        log::warn!("invalid ExMode value: ${:02X}", val);
                        self.regs.exmode
                    }
                }
            }
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
                self.mirroring = match val {
                    0x50 => Mirroring::Horizontal,
                    0x44 => Mirroring::Vertical,
                    0x00 => Mirroring::SingleScreenA,
                    0x55 => Mirroring::SingleScreenB,
                    // While the below technically isn't true - it forces my implementation to
                    // rely on the Mapper for reading Nametables in any other mode for the missing
                    // two nametables
                    _ => Mirroring::FourScreen,
                };
            }
            0x5106 => self.regs.fill.tile = val, // [TTTT TTTT] Fill Tile
            0x5107 => self.regs.fill.attr = val & 0x03, // [.... ..AA] Fill Attribute bits
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
                self.regs.chr_banks[bank] = val as usize;
                if addr < 0x5128 {
                    self.update_chr_banks(ChrBank::Spr);
                } else {
                    // Mirroring BG
                    self.regs.chr_banks[bank + 4] = self.regs.chr_banks[bank];
                    self.update_chr_banks(ChrBank::Bg);
                }
            }
            0x5130 => self.regs.chr_hi = (val as usize & 0x03) << 8, // [.... ..HH]  CHR Bank Hi bits
            0x5200 => {
                // [ES.T TTTT]    Split control
                //   E = Enable  (0=split mode disabled, 1=split mode enabled)
                //   S = Vsplit side  (0=split will be on left side, 1=split will be on right)
                //   T = tile number to split at
                self.regs.vsplit.enabled = val & 0x80 == 0x80;
                self.regs.vsplit.side = if val & 0x40 == 0x40 {
                    SplitSide::Right
                } else {
                    SplitSide::Left
                };
                self.regs.vsplit.tile = val & 0x1F;
            }
            0x5201 => self.regs.vsplit.scroll = val, // [YYYY YYYY]  Split Y scroll
            0x5202 => self.regs.vsplit.bank = val,   // [CCCC CCCC]  4k CHR Page for split
            0x5203 => self.regs.irq_scanline = u16::from(val), // [IIII IIII]  IRQ Target
            0x5204 => self.regs.irq_enabled = val & 0x80 > 0, // [E... ....] IRQ Enable (0=disabled, 1=enabled)
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
            // TODO MMC5A only CL3 / SL3 Data Direction and Output Data Source
            // TODO MMC5A only CL3 / SL3 Status
            // TODO MMC5A only 6-bit Hardware Timer with IRQ
            0x5207 | 0x5208 | 0x5209 => {}
            0x5C00..=0x5FFF => {
                let addr = addr - 0x5C00;
                match self.regs.exmode {
                    ExMode::Nametable | ExMode::Attr => {
                        if self.ppu_status.rendering {
                            self.exram.write(addr, val);
                        } else {
                            self.exram.write(addr, 0x00);
                        }
                    }
                    ExMode::Ram => self.exram.write(addr, val),
                    ExMode::RamProtected => (),
                }
            }
            0x6000..=0xDFFF if !self.rom_select(addr) => {
                return MappedWrite::PrgRam(self.prg_ram_banks.translate(addr), val);
            }
            // 0x0000..=0x1FFF CHR-ROM is read-only
            // 0x5800..=0x5BFF MMC5A unknown
            // 0xE000..=0xFFFF ROM is write-only
            _ => (),
        }
        MappedWrite::None
    }
}

impl Clocked for Exrom {
    #[inline]
    fn clock(&mut self) -> usize {
        if self.ppu_status.reading {
            self.ppu_status.idle = 0;
        } else {
            self.ppu_status.idle += 1;
            // 3 CPU clocks == 1 ppu clock
            if self.ppu_status.idle == 3 {
                self.ppu_status.idle = 0;
                self.ppu_status.in_frame = false;
                self.ppu_status.prev_addr = 0x0000;
            }
        }
        self.ppu_status.reading = false;
        1
    }
}

impl Powered for Exrom {
    fn reset(&mut self) {
        self.regs.prg_mode = PrgMode::Bank8k;
        self.regs.chr_mode = ChrMode::Bank1k;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unreadable_literal)]
    use crate::{
        common::tests::{compare, SLOT1},
        test_roms_adv,
    };

    #[test]
    fn prg_ram_protect() {
        use super::*;
        use crate::cart::Cart;
        for a in 0..4 {
            for b in 0..4 {
                let mut cart = Cart::new();
                cart.mapper = Exrom::load(&mut cart);

                cart.write(0x5102, a);
                cart.write(0x5103, b);
                cart.write(0x5114, 0);
                cart.write(0x6000, 0xFF);
                let val = cart.read(0x6000);
                if a == 0b10 && b == 0b01 {
                    assert_eq!(val, 0xFF, "RAM protect disabled: %{:02b}, %{:02b}", a, b);
                } else {
                    assert_eq!(val, 0x00, "RAM protect enabled: %{:02b}, %{:02b}", a, b);
                }
            }
        }
    }

    test_roms_adv!("mapper/m005_exrom", {
        (exram, 100, |frame, deck| match frame {
            6 => compare(5332254270321527531, deck, "exram_1"),
            15 => compare(5334265946839978920, deck, "exram_2"),
            40 => compare(7468220554807794760, deck, "exram_3"),
            100 => compare(5437218966963449815, deck, "exram_3"),
            _ => (),
        }),
        (basics, 40, |frame, deck| match frame {
            10 => compare(17691115586669895739, deck, "exrom_basics_1"),
            11 => deck.gamepad_mut(SLOT1).a = true, // Change Obj table
            12 => deck.gamepad_mut(SLOT1).a = false,
            14 => compare(11119197385669226295, deck, "exrom_basics_obj_table"),
            15 => deck.gamepad_mut(SLOT1).b = true, // Change BG table
            16 => deck.gamepad_mut(SLOT1).b = false,
            18 => compare(249922895281435000, deck, "exrom_basics_bg_table"),
            19 => deck.gamepad_mut(SLOT1).start = true, // Change Obj size
            20 => deck.gamepad_mut(SLOT1).start = false,
            22 => compare(17866245002922723459, deck, "exrom_basics_obj_size"),
            23 => deck.gamepad_mut(SLOT1).select = true, // Enable exram
            24 => deck.gamepad_mut(SLOT1).select = false,
            26 => compare(18138629485953179711, deck, "exrom_basics_exram"),
            27 => deck.gamepad_mut(SLOT1).up = true, // Enable fill
            28 => deck.gamepad_mut(SLOT1).up = false,
            30 => compare(7706206738498296599, deck, "exrom_basics_fill"),
            31 => deck.gamepad_mut(SLOT1).up = true, // Disable fill
            32 => deck.gamepad_mut(SLOT1).up = false,
            33 => deck.gamepad_mut(SLOT1).left = true, // Change bank left
            34 => deck.gamepad_mut(SLOT1).left = false,
            36 => compare(14482748193384078817, deck, "exrom_basics_bank_left"),
            37 => deck.gamepad_mut(SLOT1).left = true, // Change bank left
            38 => deck.gamepad_mut(SLOT1).left = false,
            40 => compare(7357806387480472925, deck, "exrom_basics_bank_right"),
            _ => (),
        }),
    });
}
