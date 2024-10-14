//! `ExROM`/`MMC5` (Mapper 5)
//!
//! <https://wiki.nesdev.com/w/index.php/ExROM>
//! <https://wiki.nesdev.com/w/index.php/MMC5>

use crate::{
    apu::{
        dmc::Dmc,
        pulse::{OutputFreq, Pulse, PulseChannel},
        PULSE_TABLE, TND_TABLE,
    },
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sample, Sram},
    cpu::{Cpu, Irq},
    mapper::{self, Mapped, MappedRead, MappedWrite, Mapper, MemMap},
    mem::Banks,
    ppu::{bus::PpuAddr, Mirroring, Ppu},
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use tracing::warn;

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
pub enum ChrBank {
    Spr,
    Bg,
}

bitflags! {
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
    #[must_use]
    pub struct ExRamRW: u8 {
        const W = 0x01;
        const R = 0x02;
        const RW = Self::R.bits() | Self::W.bits();
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct ExRamMode {
    bits: u8,
    nametable: bool,
    attr: bool,
    rw: ExRamRW,
}

impl Default for ExRamMode {
    fn default() -> Self {
        Self::new()
    }
}

impl ExRamMode {
    pub const fn new() -> Self {
        Self {
            bits: 0x00,
            nametable: false,
            attr: false,
            rw: ExRamRW::W,
        }
    }

    pub fn set(&mut self, val: u8) {
        let val = val & 0x03;
        self.bits = val;
        self.nametable = val <= 0b01;
        self.attr = val == 0b01;
        self.rw = match val {
            0b00 | 0b01 => ExRamRW::W,
            0b10 => ExRamRW::RW,
            0b11 => ExRamRW::R,
            _ => unreachable!("invalid exram_mode"),
        };
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Nametable {
    ScreenA,
    ScreenB,
    ExRam,
    Fill,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct NametableMapping {
    pub mode: u8,
    pub select: [Nametable; 4],
}

impl Default for NametableMapping {
    fn default() -> Self {
        Self::new()
    }
}

impl NametableMapping {
    pub const fn new() -> Self {
        Self {
            mode: 0x00,
            select: [Nametable::ScreenA; 4],
        }
    }

    pub fn set(&mut self, val: u8) {
        let nametable = |val: u8| match val & 0x03 {
            0 => Nametable::ScreenA,
            1 => Nametable::ScreenB,
            2 => Nametable::ExRam,
            3 => Nametable::Fill,
            _ => unreachable!("invalid Nametable value"),
        };
        self.mode = val;
        self.select = [
            nametable(val),
            nametable(val >> 2),
            nametable(val >> 4),
            nametable(val >> 6),
        ];
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Fill {
    pub tile: u8,    // $5106
    pub attr: usize, // $5107
}

impl Default for Fill {
    fn default() -> Self {
        Self::new()
    }
}

impl Fill {
    pub const fn new() -> Self {
        Self {
            attr: 0x03,
            tile: 0xFF,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct VSplit {
    pub mode: u8,      // $5200 [ES.T TTTT]
    pub enabled: bool, // $5200 [E... ....]
    pub side: Side,    // $5200 [.S.. ....]
    pub tile: u8,      // $5200 [...T TTTT]
    pub scroll: u8,    // $5201
    pub bank: u8,      // $5202
    pub in_region: bool,
}

impl Default for VSplit {
    fn default() -> Self {
        Self::new()
    }
}

impl VSplit {
    pub const fn new() -> Self {
        Self {
            mode: 0x00,
            enabled: false,
            side: Side::Left,
            tile: 0x00,
            scroll: 0x00,
            bank: 0x00,
            in_region: false,
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    pub prg_mode: PrgMode,                   // $5100
    pub chr_mode: ChrMode,                   // $5101
    pub prg_ram_protect: [u8; 2],            // $5102 - $5103
    pub exram_mode: ExRamMode,               // $5104
    pub nametable_mapping: NametableMapping, // $5105
    pub fill: Fill,                          // $5106 - $5107
    pub prg_banks: [usize; 5],               // $5113 - $5117
    pub chr_banks: [usize; 16],              // $5120 - $512B
    pub chr_hi: usize,                       // $5130
    pub vsplit: VSplit,                      // $5200 - $5202
    pub irq_scanline: u16,                   // $5203: Write $00 to disable IRQs
    pub irq_enabled: bool,                   // $5204
    pub multiplicand: u8,                    // $5205: write
    pub multiplier: u8,                      // $5206: write
    pub mult_result: u16,                    // $5205: read lo, $5206: read hi
}

impl Default for Regs {
    fn default() -> Self {
        Self::new()
    }
}

impl Regs {
    pub const fn new() -> Self {
        Self {
            prg_mode: PrgMode::Bank8k,
            chr_mode: ChrMode::Bank1k,
            prg_ram_protect: [0x00; 2],
            exram_mode: ExRamMode::new(),
            nametable_mapping: NametableMapping::new(),
            fill: Fill::new(),
            prg_banks: [0x00; 5],
            chr_banks: [0x00; 16],
            chr_hi: 0x00,
            vsplit: VSplit::new(),
            irq_scanline: 0x00,
            irq_enabled: false,
            multiplicand: 0xFF,
            multiplier: 0xFF,
            mult_result: 0xFE01, // e.g. 0xFF * 0xFF
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct IrqState {
    pub in_frame: bool,
    pub prev_addr: Option<u16>,
    pub match_count: u8,
    pub pending: bool,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct PpuStatus {
    pub fetch_count: u32,
    pub reading: bool,
    pub idle_count: u8,
    pub sprite8x16: bool, // $2000 PPUCTRL: false = 8x8, true = 8x16
    pub rendering: bool,
    pub scanline: u16,
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Exrom {
    pub regs: Regs,
    pub mirroring: Mirroring,
    pub ppu_status: PpuStatus,
    pub irq_state: IrqState,
    pub ex_ram: Vec<u8>,
    pub prg_ram_banks: Banks,
    pub prg_rom_banks: Banks,
    pub chr_banks: Banks,
    pub tile_cache: u16,
    pub last_chr_write: ChrBank,
    pub region: NesRegion,
    pub pulse1: Pulse,
    pub pulse2: Pulse,
    pub dmc: Dmc,
    pub dmc_mode: u8,
    pub cpu_cycle: usize,
    pub pulse_timer: f32,
}

impl Exrom {
    const PRG_WINDOW: usize = 0x2000;
    const PRG_RAM_SIZE: usize = 0x10000; // Provide 64K since mappers don't always specify
    const EXRAM_SIZE: usize = 0x0400;
    const CHR_WINDOW: usize = 0x0400;

    const ROM_SELECT_MASK: usize = 0x80; // High bit targets ROM bank switching
    const BANK_MASK: usize = 0x7F; // Ignore high bit for ROM select

    const SPR_FETCH_START: u32 = 64;
    const SPR_FETCH_END: u32 = 81;

    // This conveniently mirrors a 2-bit palette attribute to all four indexes
    // https://www.nesdev.org/wiki/MMC5#Fill-mode_color_($5107)
    const ATTR_MIRROR: [u8; 4] = [0x00, 0x55, 0xAA, 0xFF];

    // // TODO: See about generating these using oncecell
    // const ATTR_LOC: [u8; 256] = [
    //     0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
    //     0x07, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
    //     0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
    //     0x0D, 0x0E, 0x0F, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x08, 0x09, 0x0A, 0x0B,
    //     0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x10, 0x11, 0x12,
    //     0x13, 0x14, 0x15, 0x16, 0x17, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x10, 0x11,
    //     0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x18,
    //     0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
    //     0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26,
    //     0x27, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25,
    //     0x26, 0x27, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C,
    //     0x2D, 0x2E, 0x2F, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x28, 0x29, 0x2A, 0x2B,
    //     0x2C, 0x2D, 0x2E, 0x2F, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32,
    //     0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31,
    //     0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38,
    //     0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F,
    //     0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E,
    //     0x3F,
    // ];
    // const ATTR_SHIFT: [u8; 128] = [
    //     0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0,
    //     2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2,
    //     0, 0, 2, 2, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4,
    //     6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6, 4, 4, 6, 6,
    //     4, 4, 6, 6, 4, 4, 6, 6,
    // ];

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        cart.add_prg_ram(Self::PRG_RAM_SIZE);

        let mut exrom = Self {
            regs: Regs::new(),
            mirroring: cart.mirroring(),
            irq_state: IrqState {
                in_frame: false,
                prev_addr: None,
                match_count: 0,
                pending: false,
            },
            ppu_status: PpuStatus {
                fetch_count: 0x00,
                reading: false,
                idle_count: 0x00,
                sprite8x16: false,
                rendering: false,
                scanline: 0x0000,
            },
            // Cart provides an `add_ex_ram` method used by the PpuBus, but during reads from the
            // PpuBus we need access to it for bank selection so we need to store it here instead.
            ex_ram: vec![0x00; Self::EXRAM_SIZE],
            prg_ram_banks: Banks::new(0x6000, 0xFFFF, cart.prg_ram.len(), Self::PRG_WINDOW)?,
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_WINDOW)?,
            chr_banks: Banks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_WINDOW)?,
            tile_cache: 0,
            last_chr_write: ChrBank::Spr,
            region: cart.region(),
            pulse1: Pulse::new(PulseChannel::One, OutputFreq::Ultrasonic),
            pulse2: Pulse::new(PulseChannel::Two, OutputFreq::Ultrasonic),
            dmc: Dmc::new(cart.region()),
            dmc_mode: 0x01, // Default to read mode
            cpu_cycle: 0,
            pulse_timer: 0.0,
        };
        exrom.regs.prg_banks[4] = exrom.prg_rom_banks.last() | Self::ROM_SELECT_MASK;
        exrom.update_prg_banks();
        Ok(exrom.into())
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
    pub fn update_prg_banks(&mut self) {
        let mode = self.regs.prg_mode;
        let banks = self.regs.prg_banks;

        self.prg_ram_banks.set(0, banks[0]); // $5113 always selects RAM
        match mode {
            // $5117 always selects ROM
            PrgMode::Bank32k => self.prg_rom_banks.set_range(0, 3, banks[4]),
            PrgMode::Bank16k => {
                self.set_prg_bank_range(0, 1, banks[2]);
                self.prg_rom_banks
                    .set_range(2, 3, banks[4] & Self::BANK_MASK);
            }
            PrgMode::Bank16_8k => {
                self.set_prg_bank_range(0, 1, banks[2]);
                self.set_prg_bank_range(2, 2, banks[3]);
                self.prg_rom_banks.set(3, banks[4] & Self::BANK_MASK);
            }
            PrgMode::Bank8k => {
                self.set_prg_bank_range(0, 0, banks[1]);
                self.set_prg_bank_range(1, 1, banks[2]);
                self.set_prg_bank_range(2, 2, banks[3]);
                self.prg_rom_banks.set(3, banks[4] & Self::BANK_MASK);
            }
        };
    }

    pub fn set_prg_bank_range(&mut self, start: usize, end: usize, bank: usize) {
        let rom = bank & Self::ROM_SELECT_MASK == Self::ROM_SELECT_MASK;
        let bank = bank & Self::BANK_MASK;
        if rom {
            self.prg_rom_banks.set_range(start, end, bank);
        } else {
            self.prg_ram_banks.set_range(start + 1, end + 1, bank);
        }
    }

    pub fn rom_select(&self, addr: u16) -> bool {
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
            bank & Self::ROM_SELECT_MASK == Self::ROM_SELECT_MASK
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
    pub fn update_chr_banks(&mut self, chr_bank: ChrBank) {
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

    pub fn read_ex_ram(&self, addr: u16) -> u8 {
        self.ex_ram[(addr & 0x03FF) as usize]
    }

    pub fn write_ex_ram(&mut self, addr: u16, val: u8) {
        self.ex_ram[(addr & 0x03FF) as usize] = val;
    }

    pub fn inc_fetch_count(&mut self) {
        self.ppu_status.fetch_count += 1;
    }

    pub const fn fetch_count(&self) -> u32 {
        self.ppu_status.fetch_count
    }

    pub const fn sprite8x16(&self) -> bool {
        self.ppu_status.sprite8x16
    }

    pub fn spr_fetch(&self) -> bool {
        (Self::SPR_FETCH_START..Self::SPR_FETCH_END).contains(&self.fetch_count())
    }

    pub const fn nametable_select(&self, addr: u16) -> Nametable {
        self.regs.nametable_mapping.select[((addr >> 10) & 0x03) as usize]
    }
}

impl Mapped for Exrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }

    fn cpu_bus_write(&mut self, addr: u16, val: u8) {
        match addr {
            0x2000 => self.ppu_status.sprite8x16 = val & 0x20 > 0,
            0x2001 => {
                self.ppu_status.rendering = val & 0x18 > 0; // BG or Spr rendering enabled
                if !self.ppu_status.rendering {
                    self.irq_state.in_frame = false;
                    self.irq_state.prev_addr = None;
                }
            }
            _ => (),
        }
    }
}

impl MemMap for Exrom {
    // CHR mode 0
    // PPU $0000..=$1FFF 8K switchable CHR bank
    //
    // CHR mode 1
    // PPU $0000..=$0FFF 4K switchable CHR bank
    // PPU $1000..=$1FFF 4K switchable CHR bank
    //
    // CHR mode 2
    // PPU $0000..=$07FF 2K switchable CHR bank
    // PPU $0800..=$0FFF 2K switchable CHR bank
    // PPU $1000..=$17FF 2K switchable CHR bank
    // PPU $1800..=$1FFF 2K switchable CHR bank
    //
    // CHR mode 3
    // PPU $0000..=$03FF 1K switchable CHR bank
    // PPU $0400..=$07FF 1K switchable CHR bank
    // PPU $0800..=$0BFF 1K switchable CHR bank
    // PPU $0C00..=$0FFF 1K switchable CHR bank
    // PPU $1000..=$13FF 1K switchable CHR bank
    // PPU $1400..=$17FF 1K switchable CHR bank
    // PPU $1800..=$1BFF 1K switchable CHR bank
    // PPU $1C00..=$1FFF 1K switchable CHR bank
    //
    // PPU $2000..=$3EFF Up to 3 Nametables + Fill mode
    //
    // PRG mode 0
    // CPU $6000..=$7FFF 8K switchable PRG RAM bank
    // CPU $8000..=$FFFF 32K switchable PRG ROM bank
    //
    // PRG mode 1
    // CPU $6000..=$7FFF 8K switchable PRG RAM bank
    // CPU $8000..=$BFFF 16K switchable PRG ROM/RAM bank
    // CPU $C000..=$FFFF 16K switchable PRG ROM bank
    //
    // PRG mode 2
    // CPU $6000..=$7FFF 8K switchable PRG RAM bank
    // CPU $8000..=$BFFF 16K switchable PRG ROM/RAM bank
    // CPU $C000..=$DFFF 8K switchable PRG ROM/RAM bank
    // CPU $E000..=$FFFF 8K switchable PRG ROM bank
    //
    // PRG mode 3
    // CPU $6000..=$7FFF 8K switchable PRG RAM bank
    // CPU $8000..=$9FFF 8K switchable PRG ROM/RAM bank
    // CPU $A000..=$BFFF 8K switchable PRG ROM/RAM bank
    // CPU $C000..=$DFFF 8K switchable PRG ROM/RAM bank
    // CPU $E000..=$FFFF 8K switchable PRG ROM bank

    fn map_read(&mut self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => {
                self.inc_fetch_count();
                if self.sprite8x16() {
                    match self.fetch_count() {
                        Self::SPR_FETCH_START => self.update_chr_banks(ChrBank::Spr),
                        Self::SPR_FETCH_END => self.update_chr_banks(ChrBank::Bg),
                        _ => (),
                    }
                }
            }
            0x2000..=0x3EFF => {
                let is_attr = addr.is_attr();
                // Cache BG tile fetch for later attribute byte fetch
                if self.regs.exram_mode.attr && !is_attr && !self.spr_fetch() {
                    self.tile_cache = addr & 0x03FF;
                }

                // TODO: Detect split
                // if self.regs.vsplit.in_region && !is_attr {
                //     self.regs.vsplit.tile = ((self.regs.vsplit.scroll & 0xF8) << 2)
                //         | ((self.fetch_count() / 4) & 0x1F) as u8;
                // }

                // Monitor tile fetches to trigger IRQs
                // https://wiki.nesdev.org/w/index.php?title=MMC5#Scanline_Detection_and_Scanline_IRQ
                let status = &mut self.ppu_status;
                let irq_state = &mut self.irq_state;
                // Wait for three consecutive fetches to match the same address, which means we're
                // at the end of the render scanlines fetching dummy NT bytes
                if addr <= 0x2FFF && Some(addr) == irq_state.prev_addr {
                    irq_state.match_count += 1;
                    status.fetch_count = 0;
                    if irq_state.match_count == 2 {
                        if irq_state.in_frame {
                            // Scanline IRQ detected
                            status.scanline += 1;
                            if status.scanline == self.regs.irq_scanline {
                                irq_state.pending = true;
                                if self.regs.irq_enabled {
                                    Cpu::set_irq(Irq::MAPPER);
                                }
                            }
                        } else {
                            irq_state.in_frame = true;
                            status.scanline = 0;
                        }
                    }
                } else {
                    irq_state.match_count = 0;
                }
                irq_state.prev_addr = Some(addr);
                status.reading = true;
            }
            0xFFFA | 0xFFFB => {
                self.irq_state.in_frame = false; // NMI clears in_frame
                self.irq_state.prev_addr = None;
                self.irq_state.pending = false;
                Cpu::clear_irq(Irq::MAPPER);
            }
            _ => (),
        }
        let val = self.map_peek(addr);
        match addr {
            0x5204 => {
                self.irq_state.pending = false;
                Cpu::clear_irq(Irq::MAPPER);
            }
            0x5010 => Cpu::clear_irq(Irq::DMC),
            _ => (),
        }
        val
    }

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => {
                if self.regs.exram_mode.attr && !self.spr_fetch() {
                    // Bits 6-7 of 4K CHR bank. Already shifted left by 8
                    let bank_hi = self.regs.chr_hi << 10;
                    // Bits 0-5 of 4k CHR bank
                    let bank_lo = ((self.read_ex_ram(self.tile_cache) & 0x3F) as usize) << 12;
                    let addr = bank_hi | bank_lo | (addr as usize) & 0x0FFF;
                    MappedRead::Chr(addr)
                } else {
                    MappedRead::Chr(self.chr_banks.translate(addr))
                }
            }
            0x2000..=0x3EFF => {
                let is_attr = addr.is_attr();
                // TODO: vsplit
                // if self.regs.vsplit.in_region {
                //     if is_attr {
                //         // let addr =
                //         //     Self::ATTR_OFFSET | u16::from(ATTR_LOC[(self.regs.vsplit.tile as usize) >> 2]);
                //         // let attr = self.read_exram(addr - 0x2000) as usize;
                //         // let shift = ATTR_SHIFT[(self.regs.vsplit.tile as usize) & 0x7F] as usize;
                //         // MappedRead::Data(ATTR_BITS[(attr >> shift) & 0x03])
                //     } else {
                //         MappedRead::Data(self.read_exram(self.regs.vsplit.tile.into()))
                //     }
                // }
                if self.regs.exram_mode.attr && is_attr && !self.spr_fetch() {
                    // ExAttr mode returns attr bits for all nametables, regardless of mapping
                    let attr = (self.read_ex_ram(self.tile_cache) >> 6) & 0x03;
                    MappedRead::Data(Self::ATTR_MIRROR[attr as usize])
                } else {
                    let nametable_mode = self.regs.exram_mode.nametable;
                    match self.nametable_select(addr) {
                        Nametable::ScreenA => MappedRead::CIRam((addr & 0x03FF).into()),
                        Nametable::ScreenB => {
                            MappedRead::CIRam((Ppu::NT_SIZE | (addr & 0x03FF)).into())
                        }
                        Nametable::ExRam if nametable_mode => {
                            MappedRead::Data(self.read_ex_ram(addr))
                        }
                        Nametable::Fill if nametable_mode => MappedRead::Data(if is_attr {
                            Self::ATTR_MIRROR[self.regs.fill.attr & 0x03]
                        } else {
                            self.regs.fill.tile
                        }),
                        // If nametable mode is not set, zero is read back
                        _ => MappedRead::Data(0x00),
                    }
                }
            }
            0x5010 => {
                // [I... ...M] DMC
                // I = IRQ (0 = No IRQ triggered. 1 = IRQ was triggered.) Reading $5010 acknowledges the IRQ and clears this flag.
                // M = Mode select (0 = write mode. 1 = read mode.)
                let irq = Cpu::has_irq(Irq::DMC);
                MappedRead::Data(u8::from(irq) << 7 | self.dmc_mode)
            }
            0x5100 => MappedRead::Data(self.regs.prg_mode as u8),
            0x5101 => MappedRead::Data(self.regs.chr_mode as u8),
            0x5104 => MappedRead::Data(self.regs.exram_mode.bits),
            0x5105 => MappedRead::Data(self.regs.nametable_mapping.mode),
            0x5106 => MappedRead::Data(self.regs.fill.tile),
            0x5107 => MappedRead::Data(self.regs.fill.attr as u8),
            0x5015 => {
                // [.... ..BA]   Length status for Pulse 1 (A), 2 (B)
                let mut status = 0x00;
                if self.pulse1.length.counter > 0 {
                    status |= 0x01;
                }
                if self.pulse2.length.counter > 0 {
                    status |= 0x02;
                }
                MappedRead::Data(status)
            }
            0x5113..=0x5117 => {
                MappedRead::Data(self.regs.prg_banks[(addr - 0x5113) as usize] as u8)
            }
            0x5120..=0x512B => {
                MappedRead::Data(self.regs.chr_banks[(addr - 0x5120) as usize] as u8)
            }
            0x5130 => MappedRead::Data(self.regs.chr_hi as u8),
            0x5200 => MappedRead::Data(self.regs.vsplit.mode),
            0x5201 => MappedRead::Data(self.regs.vsplit.scroll),
            0x5202 => MappedRead::Data(self.regs.vsplit.bank),
            0x5203 => MappedRead::Data(self.regs.irq_scanline as u8),
            0x5204 => {
                // $5204:  [PI.. ....]
                //   P = IRQ currently pending
                //   I = "In Frame" signal

                let irq_pending = Cpu::has_irq(Irq::MAPPER);
                // Reading $5204 will clear the pending flag (acknowledging the IRQ).
                // Clearing is done in the read() function
                MappedRead::Data(
                    u8::from(irq_pending) << 7 | u8::from(self.irq_state.in_frame) << 6,
                )
            }
            0x5205 => MappedRead::Data((self.regs.mult_result & 0xFF) as u8),
            0x5206 => MappedRead::Data(((self.regs.mult_result >> 8) & 0xFF) as u8),
            0x5C00..=0x5FFF if self.regs.exram_mode.rw != ExRamRW::W => {
                // Nametable/Attr modes are not used for RAM, thus are not readable
                MappedRead::Data(self.read_ex_ram(addr))
            }
            0x6000..=0xDFFF => {
                if self.rom_select(addr) {
                    MappedRead::PrgRom(self.prg_rom_banks.translate(addr))
                } else {
                    MappedRead::PrgRam(self.prg_ram_banks.translate(addr))
                }
            }
            0xE000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            0x5207..=0x5209 => MappedRead::Data(0),
            _ => MappedRead::Bus,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x2000..=0x3EFF => match self.nametable_select(addr) {
                Nametable::ScreenA => return MappedWrite::CIRam((addr & 0x03FF).into(), val),
                Nametable::ScreenB => {
                    return MappedWrite::CIRam((Ppu::NT_SIZE | (addr & 0x03FF)).into(), val)
                }
                Nametable::ExRam if self.regs.exram_mode.nametable => {
                    self.write_ex_ram(addr, val);
                    return MappedWrite::None;
                }
                _ => return MappedWrite::None,
            },
            0x5000 => self.pulse1.write_ctrl(val),
            // 0x5001 Has no effect since there is no Sweep unit
            0x5002 => self.pulse1.write_timer_lo(val),
            0x5003 => self.pulse1.write_timer_hi(val),
            0x5004 => self.pulse2.write_ctrl(val),
            // 0x5005 Has no effect since there is no Sweep unit
            0x5006 => self.pulse2.write_timer_lo(val),
            0x5007 => self.pulse2.write_timer_hi(val),
            0x5010 => {
                // [I... ...M] DMC
                //   I = PCM IRQ enable (1 = enabled.)
                //   M = Mode select (0 = write mode. 1 = read mode.)
                self.dmc_mode = val & 0x01;
                self.dmc.irq_enabled = val & 0x80 == 0x80;
            }
            0x5011 => {
                // [DDDD DDDD] PCM Data
                // Write mode - writing $00 has no effect
                if self.dmc_mode == 0 && val != 0x00 {
                    self.dmc.write_output(val);
                }
            }
            0x5015 => {
                //  [.... ..BA]   Enable flags for Pulse 1 (A), 2 (B)  (0=disable, 1=enable)
                self.pulse1.set_enabled(val & 0x01 == 0x01);
                self.pulse2.set_enabled(val & 0x02 == 0x02);
            }
            0x5100 => {
                // [.... ..PP] PRG Mode
                self.regs.prg_mode = match val & 0x03 {
                    0 => PrgMode::Bank32k,
                    1 => PrgMode::Bank16k,
                    2 => PrgMode::Bank16_8k,
                    3 => PrgMode::Bank8k,
                    _ => {
                        warn!("invalid PrgMode value: ${:02X}", val);
                        self.regs.prg_mode
                    }
                };
                self.update_prg_banks();
            }
            0x5101 => {
                // [.... ..CC] CHR Mode
                if self.regs.exram_mode.attr {
                    // Bank switching is ignored in extended attribute mode, banks are always 4K
                    self.regs.chr_mode = ChrMode::Bank4k;
                } else {
                    self.regs.chr_mode = match val & 0x03 {
                        0 => ChrMode::Bank8k,
                        1 => ChrMode::Bank4k,
                        2 => ChrMode::Bank2k,
                        3 => ChrMode::Bank1k,
                        _ => {
                            warn!("invalid ChrMode value: ${:02X}", val);
                            self.regs.chr_mode
                        }
                    };
                }
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
                // [.... ..XX] ExRam mode
                //   Value  RAM $5C00-$5FFF  RAM Nametable  Extended Attr
                //   %00    Write Only       Yes            No
                //   %01    Write Only       Yes            Yes
                //   %10    Read/Write       No             No
                //   %11    Read Only        No             No
                self.regs.exram_mode.set(val);
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
                self.regs.nametable_mapping.set(val);

                // Typical mirroring setups would be:
                //                          D  C  B  A
                //   Horizontal:     $50    01 01 00 00
                //   Vertical:       $44    01 00 01 00
                //   SingleScreenA:  $00    00 00 00 00
                //   SingleScreenB:  $55    01 01 01 01
                //   SingleScreen ExRAM:   $AA    10 10 10 10
                //   SingleScreen Fill:    $FF    11 11 11 11
                self.mirroring = match val {
                    0x50 => Mirroring::Horizontal,
                    0x44 => Mirroring::Vertical,
                    0x00 => Mirroring::SingleScreenA,
                    0x55 => Mirroring::SingleScreenB,
                    // Any other combination means Mapper provides nametables
                    _ => Mirroring::FourScreen,
                };
            }
            0x5106 => self.regs.fill.tile = val, // [TTTT TTTT] Fill Tile
            0x5107 => self.regs.fill.attr = (val & 0x03).into(), // [.... ..AA] Fill Attribute bits
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
                    Side::Right
                } else {
                    Side::Left
                };
                self.regs.vsplit.tile = val & 0x1F;
            }
            0x5201 => self.regs.vsplit.scroll = val, // [YYYY YYYY]  Split Y scroll
            0x5202 => self.regs.vsplit.bank = val,   // [CCCC CCCC]  4k CHR Page for split
            0x5203 => self.regs.irq_scanline = u16::from(val), // [IIII IIII]  IRQ Target
            0x5204 => {
                self.regs.irq_enabled = val & 0x80 > 0; // [E... ....] IRQ Enable (0=disabled, 1=enabled)
                if !self.regs.irq_enabled {
                    Cpu::clear_irq(Irq::MAPPER);
                } else if self.irq_state.pending {
                    Cpu::set_irq(Irq::MAPPER);
                }
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
            0x5207..=0x5209 => {}
            0x5C00..=0x5FFF => match self.regs.exram_mode.rw {
                ExRamRW::W => {
                    let val = if self.ppu_status.rendering { val } else { 0x00 };
                    self.write_ex_ram(addr, val);
                }
                ExRamRW::RW => self.write_ex_ram(addr, val),
                _ => (),
            },
            0x6000..=0xDFFF if !self.rom_select(addr) => {
                return MappedWrite::PrgRam(self.prg_ram_banks.translate(addr), val);
            }
            _ => (),
        }
        MappedWrite::Bus
    }
}

impl Reset for Exrom {
    fn reset(&mut self, _kind: ResetKind) {
        self.regs.prg_mode = PrgMode::Bank8k;
        self.regs.chr_mode = ChrMode::Bank1k;
    }
}

impl Clock for Exrom {
    fn clock(&mut self) -> usize {
        if self.ppu_status.reading {
            self.ppu_status.idle_count = 0;
        } else {
            self.ppu_status.idle_count += 1;
            // 3 CPU clocks == 1 ppu clock
            if self.ppu_status.idle_count == 3 {
                self.ppu_status.idle_count = 0;
                self.irq_state.in_frame = false;
                self.irq_state.prev_addr = None;
            }
        }
        self.ppu_status.reading = false;

        self.pulse1.clock();
        self.pulse2.clock();
        self.dmc.clock();
        self.pulse_timer -= 1.0;
        if self.pulse_timer <= 0.0 {
            self.pulse1.clock_half_frame();
            self.pulse2.clock_half_frame();
            self.pulse_timer = Cpu::region_clock_rate(self.region) / 240.0;
        }

        self.pulse1.length.reload();
        self.pulse2.length.reload();

        self.cpu_cycle = self.cpu_cycle.wrapping_add(1);
        1
    }
}

impl Regional for Exrom {
    fn region(&self) -> NesRegion {
        self.dmc.region()
    }

    fn set_region(&mut self, region: NesRegion) {
        self.dmc.set_region(region);
    }
}

impl Sram for Exrom {}

impl Sample for Exrom {
    #[must_use]
    fn output(&self) -> f32 {
        let pulse1 = self.pulse1.output();
        let pulse2 = self.pulse2.output();
        let pulse = PULSE_TABLE[(pulse1 + pulse2) as usize];
        let dmc = TND_TABLE[self.dmc.output() as usize];
        -(pulse + dmc)
    }
}

impl std::fmt::Debug for Exrom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Exrom")
            .field("regs", &self.regs)
            .field("mirroring", &self.mirroring)
            .field("ppu_status", &self.ppu_status)
            .field("irq_state", &self.irq_state)
            .field("exram_len", &self.ex_ram.len())
            .field("prg_ram_banks", &self.prg_ram_banks)
            .field("prg_rom_banks", &self.prg_rom_banks)
            .field("chr_banks", &self.chr_banks)
            .field("tile_cache", &self.tile_cache)
            .field("last_chr_write", &self.last_chr_write)
            .field("region", &self.region)
            .field("pulse1", &self.pulse1)
            .field("pulse2", &self.pulse2)
            .field("dmc", &self.dmc)
            .field("dmc_mode", &self.dmc_mode)
            .field("cpu_cycle", &self.cpu_cycle)
            .field("pulse_timer", &self.pulse_timer)
            .finish()
    }
}
