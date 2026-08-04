//! `ExROM`/`MMC5` (Mapper 5).
//!
//! <https://wiki.nesdev.org/w/index.php/ExROM>
//! <https://wiki.nesdev.org/w/index.php/MMC5>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    apu::{
        PULSE_TABLE, TND_TABLE,
        dmc::Dmc,
        pulse::{OutputFreq, Pulse, PulseChannel},
    },
    cart::Cart,
    common::{NesRegion, ResetKind},
    cpu::Cpu,
    mapper::{self, Map, Mapper, MapperOps},
    memory::{Memory, Src},
    ppu::{self, Mirroring},
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// PRG banking mode.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum PrgMode {
    Bank32k,
    Bank16k,
    Bank16_8k,
    Bank8k,
}

/// CHR banking mode.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum ChrMode {
    Bank8k,
    Bank4k,
    Bank2k,
    Bank1k,
}

/// CHR bank registers.
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

/// Exram mode registers.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct ExRamMode {
    pub bits: u8,
    pub nametable: bool,
    pub attr: bool,
    pub rw: ExRamRW,
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

    pub const fn set(&mut self, val: u8) {
        let val = val & 0b11;
        self.bits = val;
        self.nametable = val <= 0b01;
        self.attr = val == 0b01;
        self.rw = match val {
            0b00 | 0b01 => ExRamRW::W,
            0b10 => ExRamRW::RW,
            _ => ExRamRW::R,
        };
    }
}

/// Exram nametable select.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Nametable {
    ScreenA,
    ScreenB,
    ExRam,
    Fill,
}

/// Exram nametable mapping registers.
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
        let nametable = |val: u8| match val & 0b11 {
            0b00 => Nametable::ScreenA,
            0b01 => Nametable::ScreenB,
            0b10 => Nametable::ExRam,
            _ => Nametable::Fill,
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

/// Exram fill registers.
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

/// Vertical split side.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Side {
    Left,
    Right,
}

/// Vertical split mode.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct VSplit {
    pub mode: u8,      // $5200 [ES.T TTTT]
    pub enabled: bool, // $5200 [E... ....]
    pub side: Side,    // $5200 [.S.. ....]
    pub tile: u8,      // $5200 [...T TTTT]
    pub scroll: u8,    // $5201
    pub bank: u8,      // $5202
    /// Whether the tile currently being fetched falls inside the split region.
    pub in_region: bool,
    /// ExRAM offset of the split tile the last nametable fetch selected. The attribute and
    /// pattern fetches that follow it are derived from this rather than from the PPU's address.
    pub tile_addr: u16,
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
            tile_addr: 0x0000,
        }
    }
}

/// `ExROM` registers.
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
    pub irq_pending: bool,
    pub multiplicand: u8, // $5205: write
    pub multiplier: u8,   // $5206: write
    pub mult_result: u16, // $5205: read lo, $5206: read hi
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
            irq_pending: false,
            multiplicand: 0xFF,
            multiplier: 0xFF,
            mult_result: 0xFE01, // e.g. 0xFF * 0xFF
        }
    }
}

/// `ExROM` IRQ state.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct IrqState {
    pub in_frame: bool,
    pub prev_addr: Option<u16>,
    pub match_count: u8,
    pub pending: bool,
}

/// Internally tracked PPU status.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct PpuStatus {
    /// Nametable fetches so far this scanline, counting the garbage fetches during sprite
    /// evaluation and the two prefetched tiles.
    ///
    /// It says which screen column a fetch belongs to, which is what the vertical split needs,
    /// and which part of the scanline the PPU is in, which is what tells sprite fetches from
    /// background fetches.
    pub tile_number: u32,
    pub reading: bool,
    pub idle_count: u8,
    pub sprite8x16: bool, // $2000 PPUCTRL: false = 8x8, true = 8x16
    pub rendering: bool,
    pub scanline: u16,
}

/// `ExROM`/`MMC5` (Mapper 5).
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Exrom {
    pub regs: Regs,
    pub mirroring: Mirroring,
    pub ppu_status: PpuStatus,
    pub irq_state: IrqState,
    pub tile_cache: u16,
    /// Which of the two CHR bank sets the page tables currently hold.
    pub chr_set: ChrBank,
    /// Which set the last CHR register write belonged to, which decides the set outside a
    /// rendered frame - there is no scanline to key on then.
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
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    const EX_RAM_WINDOW: usize = 1024;

    const ROM_SELECT_MASK: usize = 0x80; // High bit targets ROM bank switching
    const BANK_MASK: usize = 0x7F; // Ignore high bit for ROM select
    /// PRG-RAM is emulated as a single 64K block, i.e. eight 8K banks.
    const PRG_RAM_BANK_MASK: usize = 0x07;

    /// Nametable fetches 32-39 of a scanline are the eight garbage fetches the PPU makes while
    /// fetching sprite patterns.
    const SPR_TILE_START: u32 = 32;
    const SPR_TILE_END: u32 = 40;

    /// Nametable fetches in a scanline: 32 visible tiles, 8 garbage fetches during sprite
    /// evaluation, and 2 tiles prefetched for the next scanline.
    const SPLIT_COLUMNS: u32 = 42;

    // This conveniently mirrors a 2-bit palette attribute to all four indexes
    // https://www.nesdev.org/wiki/MMC5#Fill-mode_color_($5107)
    const ATTR_MIRROR: [u8; 4] = [0x00, 0x55, 0xAA, 0xFF];

    /// Load `Exrom` from `Cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
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
                tile_number: 0x00,
                reading: false,
                idle_count: 0x00,
                sprite8x16: false,
                rendering: false,
                scanline: 0x0000,
            },
            tile_cache: 0,
            chr_set: ChrBank::Spr,
            last_chr_write: ChrBank::Spr,
            region: cart.region,
            pulse1: Pulse::new(PulseChannel::One, OutputFreq::Ultrasonic),
            pulse2: Pulse::new(PulseChannel::Two, OutputFreq::Ultrasonic),
            dmc: Dmc::new(cart.region),
            dmc_mode: 0x01, // Default to read mode
            cpu_cycle: 0,
            pulse_timer: 0.0,
        };
        // "Games seem to expect $5117 to be $FF on powerup (last PRG page swapped in)."
        let last_prg_bank = (cart.memory.region_ref(Src::PrgRom).len() / Self::PRG_WINDOW)
            .saturating_sub(1)
            & Self::BANK_MASK;
        exrom.regs.prg_banks[4] = last_prg_bank | Self::ROM_SELECT_MASK;
        exrom.update_banks(&mut cart.memory);
        Ok(exrom.into())
    }

    /// Whether PRG-RAM currently accepts writes.
    ///
    /// $5102 must hold %10 and $5103 %01; any other combination write-protects it.
    const fn prg_ram_writable(&self) -> bool {
        self.regs.prg_ram_protect[0] == 0x02 && self.regs.prg_ram_protect[1] == 0x01
    }

    /// Map one 8K PRG window from a bank register value.
    ///
    /// Bit 7 of $5114-$5116 picks ROM over RAM. $5113 is always RAM and $5117 always ROM, which
    /// callers force with `rom`.
    fn map_prg_bank(&self, memory: &mut Memory, addr: u16, val: usize, rom: bool) {
        if rom || val & Self::ROM_SELECT_MASK == Self::ROM_SELECT_MASK {
            memory.map_prg(
                addr,
                Self::PRG_WINDOW,
                (val & Self::BANK_MASK) as i32,
                Src::PrgRom,
            );
        } else {
            memory.map_prg(
                addr,
                Self::PRG_WINDOW,
                (val & Self::PRG_RAM_BANK_MASK) as i32,
                Src::PrgRam,
            );
            memory.set_prg_writable(addr, Self::PRG_WINDOW, self.prg_ram_writable());
        }
    }

    /// Map a 16K PRG window as two 8K pages. The low bank bit is ignored, as on hardware.
    fn map_prg_bank_16k(&self, memory: &mut Memory, addr: u16, val: usize, rom: bool) {
        let select = val & Self::ROM_SELECT_MASK;
        let bank = val & Self::BANK_MASK & !0x01;
        self.map_prg_bank(memory, addr, select | bank, rom);
        self.map_prg_bank(memory, addr + 0x2000, select | (bank + 1), rom);
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
    fn update_prg_banks(&self, memory: &mut Memory) {
        let banks = self.regs.prg_banks;
        // $5113 always selects PRG-RAM.
        self.map_prg_bank(memory, 0x6000, banks[0] & Self::PRG_RAM_BANK_MASK, false);
        match self.regs.prg_mode {
            PrgMode::Bank32k => {
                // A 32K window, so the low two bank bits are ignored.
                let base = banks[4] & Self::BANK_MASK & !0x03;
                for slot in 0..4 {
                    let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
                    self.map_prg_bank(memory, addr, base + slot, true);
                }
            }
            PrgMode::Bank16k => {
                self.map_prg_bank_16k(memory, 0x8000, banks[2], false);
                self.map_prg_bank_16k(memory, 0xC000, banks[4], true);
            }
            PrgMode::Bank16_8k => {
                self.map_prg_bank_16k(memory, 0x8000, banks[2], false);
                self.map_prg_bank(memory, 0xC000, banks[3], false);
                self.map_prg_bank(memory, 0xE000, banks[4], true);
            }
            PrgMode::Bank8k => {
                self.map_prg_bank(memory, 0x8000, banks[1], false);
                self.map_prg_bank(memory, 0xA000, banks[2], false);
                self.map_prg_bank(memory, 0xC000, banks[3], false);
                self.map_prg_bank(memory, 0xE000, banks[4], true);
            }
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
    fn update_chr_banks(&self, memory: &mut Memory) {
        let hi = self.regs.chr_hi;
        let banks = match self.chr_set {
            ChrBank::Spr => &self.regs.chr_banks[0..8],
            ChrBank::Bg => &self.regs.chr_banks[8..16],
        };
        // $5130's two bits extend the bank *number*, which is counted in whatever window size the
        // current CHR mode uses, so they are folded in before the shift down to 1K pages.
        let map = |memory: &mut Memory, slot: usize, count: usize, bank: usize, shift: usize| {
            let base = (bank | hi) << shift;
            for i in 0..count {
                let addr = ((slot + i) * Self::CHR_WINDOW) as u16;
                memory.map_chr(addr, Self::CHR_WINDOW, (base + i) as i32, Src::Chr);
            }
        };
        match self.regs.chr_mode {
            ChrMode::Bank8k => map(memory, 0, 8, banks[7], 3),
            ChrMode::Bank4k => {
                map(memory, 0, 4, banks[3], 2);
                map(memory, 4, 4, banks[7], 2);
            }
            ChrMode::Bank2k => {
                for (slot, reg) in [(0, 1), (2, 3), (4, 5), (6, 7)] {
                    map(memory, slot, 2, banks[reg], 1);
                }
            }
            ChrMode::Bank1k => {
                for (slot, &bank) in banks.iter().enumerate() {
                    map(memory, slot, 1, bank, 0);
                }
            }
        }
    }

    /// Point each of the four nametable slots at whatever $5105 selects for it.
    ///
    /// This is why MMC5 does not call [`Memory::set_mirroring`]: the four slots are independent
    /// and two of the four sources are not CIRAM at all.
    fn update_nametables(&self, memory: &mut Memory) {
        let nametable_mode = self.regs.exram_mode.nametable;
        for (i, select) in self.regs.nametable_mapping.select.into_iter().enumerate() {
            let addr = 0x2000 + (i * Self::CHR_WINDOW) as u16;
            // $3000-$3EFF mirrors $2000-$2EFF.
            for addr in [addr, addr + 0x1000] {
                match select {
                    Nametable::ScreenA => memory.map_chr(addr, Self::CHR_WINDOW, 0, Src::CiRam),
                    Nametable::ScreenB => memory.map_chr(addr, Self::CHR_WINDOW, 1, Src::CiRam),
                    Nametable::ExRam if nametable_mode => {
                        memory.map_chr(addr, Self::CHR_WINDOW, 0, Src::ExRam);
                    }
                    // Fill mode is synthesised by `chr_read`, and outside ExRAM nametable
                    // mode both remaining selections read back as zero - which is what an unmapped
                    // page gives. Either way there is nothing for a page entry to point at.
                    _ => memory.unmap_chr(addr, Self::CHR_WINDOW),
                }
            }
        }
    }

    /// Map ExRAM into the CPU window at $5C00-$5FFF according to the current ExRAM mode.
    fn update_ex_ram(&self, memory: &mut Memory) {
        if self.regs.exram_mode.rw.contains(ExRamRW::R) {
            memory.map_prg(0x5C00, Self::EX_RAM_WINDOW, 0, Src::ExRam);
            let writable = self.regs.exram_mode.rw.contains(ExRamRW::W);
            memory.set_prg_writable(0x5C00, Self::EX_RAM_WINDOW, writable);
        } else {
            // Modes 0 and 1 are write-only, which a page entry cannot express, so the window stays
            // unmapped (reads yield zero) and `write_register` stores the byte itself.
            memory.unmap_prg(0x5C00, Self::EX_RAM_WINDOW);
        }
    }

    /// Which CHR bank set the fetch in progress must use.
    ///
    /// With 8x8 sprites the 'B' registers are ignored entirely and everything comes from 'A'.
    /// With 8x16 sprites the PPU fetches sprite patterns from 'A' and background patterns from
    /// 'B', which the nametable-fetch counter distinguishes. Outside a rendered frame there is no
    /// scanline to key on, so the set follows whichever register was written last.
    const fn required_chr_set(&self) -> ChrBank {
        let spr_fetch = self.spr_fetch();
        let idle = !self.irq_state.in_frame && matches!(self.last_chr_write, ChrBank::Spr);
        if !self.ppu_status.sprite8x16 || spr_fetch || idle {
            ChrBank::Spr
        } else {
            ChrBank::Bg
        }
    }

    /// Latch the CHR bank set the PPU should be reading from, re-mapping only if it changed.
    ///
    /// The layer above [`Exrom::update_chr_banks`], and the reason MMC5 has one at all: every other
    /// board has a single set of CHR registers, so mapping is a pure function of them. MMC5 has two
    /// ('A' and 'B', see [`Exrom::required_chr_set`]) and switches between them *mid-frame* as the
    /// PPU alternates sprite and background fetches. This is called from the hot PPU-fetch path, so
    /// it compares first and re-maps only on a change; `force` is for the callers that have just
    /// rewritten the registers and know the mapping is stale regardless.
    fn select_chr_set(&mut self, memory: &mut Memory, force: bool) {
        if !self.ppu_status.sprite8x16 {
            // 8x8 sprites ignore $5128-$512B completely, so a write to one of them cannot leave
            // the 'B' set selected.
            self.last_chr_write = ChrBank::Spr;
        }
        let chr_set = self.required_chr_set();
        if force || chr_set != self.chr_set {
            self.chr_set = chr_set;
            self.update_chr_banks(memory);
        }
    }

    fn read_ex_ram(&self, memory: &Memory, addr: u16) -> u8 {
        memory.region_peek(Src::ExRam, (addr & 0x03FF) as usize)
    }

    pub const fn sprite8x16(&self) -> bool {
        self.ppu_status.sprite8x16
    }

    /// Whether the PPU is fetching sprite patterns rather than background ones.
    pub const fn spr_fetch(&self) -> bool {
        self.ppu_status.tile_number >= Self::SPR_TILE_START
            && self.ppu_status.tile_number < Self::SPR_TILE_END
    }

    pub const fn nametable_select(&self, addr: u16) -> Nametable {
        self.regs.nametable_mapping.select[((addr >> 10) & 0x03) as usize]
    }

    /// Whether the vertical split can affect the fetch in progress.
    ///
    /// It shares ExRAM with the nametable modes, so it only exists in ExRAM modes 0 and 1, and it
    /// tracks screen columns, so it means nothing outside a rendered frame.
    const fn split_active(&self) -> bool {
        self.regs.vsplit.enabled && self.regs.exram_mode.nametable && self.irq_state.in_frame
    }

    /// The split region's own vertical scroll for the scanline being fetched.
    ///
    /// The last two nametable fetches of a scanline prefetch the next one, so they scroll by a
    /// scanline more than the counter says.
    ///
    /// Both of them: tiles 40 and 41 are columns 0 and 1 of the next line, and `scanline` is not
    /// incremented until the scanline-detect fetch at the start of that line.
    fn split_scroll(&self) -> u16 {
        let scanline = if self.ppu_status.tile_number >= 40 {
            self.ppu_status.scanline + 1
        } else {
            self.ppu_status.scanline
        };
        (scanline + u16::from(self.regs.vsplit.scroll)) % 240
    }

    /// Screen column the nametable fetch in progress will be displayed at.
    ///
    /// The PPU fetches two tiles ahead, so the counter runs two columns behind - which also puts
    /// the two prefetched tiles of the previous scanline at columns 0 and 1.
    const fn split_column(&self) -> u32 {
        (self.ppu_status.tile_number + 2) % Self::SPLIT_COLUMNS
    }

    /// Track whether the nametable fetch in progress falls inside the split region, and which
    /// ExRAM tile it selects.
    fn update_split_region(&mut self) {
        let column = self.split_column();
        if column == 0 {
            // A fresh scanline's worth of tiles starts here, on whichever side $5200 selects.
            self.regs.vsplit.in_region = self.regs.vsplit.side == Side::Left;
        }
        if column == u32::from(self.regs.vsplit.tile)
            && self.ppu_status.tile_number < Self::SPLIT_COLUMNS
        {
            // Crossing the delimiter column swaps which side of it is being drawn.
            self.regs.vsplit.in_region = !self.regs.vsplit.in_region;
        } else if column > 32 {
            // Sprite-evaluation garbage fetches, which are not screen columns at all.
            self.regs.vsplit.in_region = false;
        }
        if self.regs.vsplit.in_region {
            self.regs.vsplit.tile_addr = ((self.split_scroll() & 0xF8) << 2) | column as u16;
        }
    }

    /// Nametable or attribute byte for a fetch inside the split region, which comes from ExRAM
    /// laid out as a nametable of its own rather than from the selected nametable source.
    fn split_nametable_read(&self, memory: &Memory, is_nt_fetch: bool) -> Option<u8> {
        if !self.split_active() || !self.regs.vsplit.in_region {
            return None;
        }
        let tile = self.regs.vsplit.tile_addr;
        Some(if is_nt_fetch {
            memory.region_peek(Src::ExRam, tile as usize)
        } else {
            let shift = ((tile >> 4) & 0x04) | (tile & 0x02);
            let attr_addr = 0x03C0 | ((tile & 0x0380) >> 4) | ((tile & 0x001F) >> 2);
            let attr = memory.region_peek(Src::ExRam, attr_addr as usize);
            Self::ATTR_MIRROR[((attr >> shift) & 0x03) as usize]
        })
    }

    /// Pattern byte for a fetch inside the split region, which uses the split's own 4K CHR bank
    /// and its own fine Y rather than the PPU's.
    fn split_chr_read(&self, memory: &Memory, addr: u16) -> Option<u8> {
        if !self.split_active() || !self.regs.vsplit.in_region {
            return None;
        }
        let bank = (self.regs.vsplit.bank as usize) << 12;
        let fine_y = self.split_scroll() as usize & 0x07;
        Some(memory.region_peek(Src::Chr, bank | ((addr as usize & !0x07) | fine_y) & 0x0FFF))
    }

    /// Pattern-table byte for a tile whose CHR bank came from ExRAM.
    ///
    /// In extended-attribute mode the bank is chosen per tile by the ExRAM byte matching the
    /// nametable entry, ignoring the CHR bank registers entirely, so no page entry can serve it.
    /// Returns `None` when normal banking applies.
    fn ex_attr_chr_read(&self, memory: &Memory, addr: u16) -> Option<u8> {
        (self.regs.exram_mode.attr && !self.spr_fetch()).then(|| {
            // Bits 6-7 of the 4K CHR bank, already shifted left by 8.
            let bank_hi = self.regs.chr_hi << 10;
            // Bits 0-5 of the 4K CHR bank.
            let bank_lo = ((self.read_ex_ram(memory, self.tile_cache) & 0x3F) as usize) << 12;
            memory.region_peek(Src::Chr, bank_hi | bank_lo | (addr as usize & 0x0FFF))
        })
    }

    /// Nametable byte for reads the page tables cannot serve: extended attributes and fill mode.
    ///
    /// Returns `None` for the ordinary CIRAM and ExRAM nametable sources, which are page entries.
    fn nametable_read(&self, memory: &Memory, addr: u16) -> Option<u8> {
        let is_attr = ppu::is_attr(addr);
        if self.regs.exram_mode.attr && is_attr && !self.spr_fetch() {
            // ExAttr mode returns attr bits for all nametables, regardless of mapping
            let attr = (self.read_ex_ram(memory, self.tile_cache) >> 6) & 0x03;
            return Some(Self::ATTR_MIRROR[attr as usize]);
        }
        match self.nametable_select(addr) {
            Nametable::Fill if self.regs.exram_mode.nametable => Some(if is_attr {
                Self::ATTR_MIRROR[self.regs.fill.attr & 0x03]
            } else {
                self.regs.fill.tile
            }),
            _ => None,
        }
    }
}

impl Map for Exrom {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED
            | MapperOps::IRQ
            | MapperOps::AUDIO
            | MapperOps::DMA
            | MapperOps::SERVES_PRG_READS
            | MapperOps::SERVES_CHR_READS
    }

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

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.regs.irq_pending || self.dmc.irq_pending
    }

    fn dma_pending(&self) -> bool {
        self.dmc.dma_pending
    }

    fn clear_dma_pending(&mut self) {
        self.dmc.dma_pending = false;
    }

    /// The register file and ExRAM in modes 0/1 are not memory, and every PPU fetch has side
    /// effects, so MMC5 is the one board that serves both kinds of read itself.
    fn prg_read(&mut self, addr: u16) -> Option<u8> {
        if let 0xFFFA | 0xFFFB = addr {
            // Reading the NMI vector clears the in-frame flag.
            self.irq_state.in_frame = false;
            self.irq_state.prev_addr = None;
            self.irq_state.pending = false;
            self.regs.irq_pending = false;
        }
        let val = self.prg_peek(addr);
        match addr {
            // Reading $5204 acknowledges the scanline IRQ, and $5010 the DMC IRQ.
            0x5204 => {
                self.irq_state.pending = false;
                self.regs.irq_pending = false;
            }
            0x5010 => self.dmc.irq_pending = false,
            _ => (),
        }
        val
    }

    fn prg_peek(&self, addr: u16) -> Option<u8> {
        let val = match addr {
            0x5010 => {
                // [I... ...M] DMC
                // I = IRQ (0 = No IRQ triggered. 1 = IRQ was triggered.) Reading $5010 acknowledges the IRQ and clears this flag.
                // M = Mode select (0 = write mode. 1 = read mode.)
                (u8::from(self.dmc.irq_pending) << 7) | self.dmc_mode
            }
            0x5015 => {
                // [.... ..BA]   Length status for Pulse 1 (A), 2 (B)
                let mut status = 0x00;
                if self.pulse1.length.counter > 0 {
                    status |= 0x01;
                }
                if self.pulse2.length.counter > 0 {
                    status |= 0x02;
                }
                status
            }
            0x5100 => self.regs.prg_mode as u8,
            0x5101 => self.regs.chr_mode as u8,
            0x5104 => self.regs.exram_mode.bits,
            0x5105 => self.regs.nametable_mapping.mode,
            0x5106 => self.regs.fill.tile,
            0x5107 => self.regs.fill.attr as u8,
            0x5113..=0x5117 => self.regs.prg_banks[(addr - 0x5113) as usize] as u8,
            0x5120..=0x512B => self.regs.chr_banks[(addr - 0x5120) as usize] as u8,
            0x5130 => self.regs.chr_hi as u8,
            0x5200 => self.regs.vsplit.mode,
            0x5201 => self.regs.vsplit.scroll,
            0x5202 => self.regs.vsplit.bank,
            0x5203 => self.regs.irq_scanline as u8,
            0x5204 => {
                // $5204:  [PI.. ....]
                //   P = IRQ currently pending
                //   I = "In Frame" signal

                // Reading $5204 will clear the pending flag (acknowledging the IRQ).
                // Clearing is done in `prg_read`.
                //
                // `irq_state.pending`, not `regs.irq_pending`: the hardware raises the pending
                // flag whether or not IRQs are enabled, and only the *assertion* to the CPU is
                // gated (`docs/mapper/005.txt:411`). A game that polls $5204 with IRQs disabled
                // reads the flag, so reporting the gated one tells it a scanline never matched.
                (u8::from(self.irq_state.pending) << 7) | (u8::from(self.irq_state.in_frame) << 6)
            }
            0x5205 => (self.regs.mult_result & 0xFF) as u8,
            0x5206 => ((self.regs.mult_result >> 8) & 0xFF) as u8,
            // ExRAM is a page entry whenever it is readable, so it never reaches here.
            _ => return None,
        };
        Some(val)
    }

    /// Every PPU fetch drives the scanline detector, the CHR bank-set switch and, in extended
    /// attribute mode, the tile lookup - so unlike every other board MMC5 serves reads rather than
    /// just watching the bus.
    fn chr_read(&mut self, memory: &mut Memory, addr: u16) -> Option<u8> {
        match addr {
            0x0000..=0x1FFF => {
                self.select_chr_set(memory, false);
                self.split_chr_read(memory, addr)
                    .or_else(|| self.ex_attr_chr_read(memory, addr))
            }
            0x2000..=0x3EFF => {
                let is_attr = ppu::is_attr(addr);
                let is_nt_fetch = addr <= 0x2FFF && !is_attr;
                if is_nt_fetch {
                    self.ppu_status.tile_number += 1;
                }
                // Cache BG tile fetch for later attribute byte fetch
                if self.regs.exram_mode.attr && !is_attr && !self.spr_fetch() {
                    self.tile_cache = addr & 0x03FF;
                }

                // Monitor tile fetches to trigger IRQs
                // https://wiki.nesdev.org/w/index.php?title=MMC5#Scanline_Detection_and_Scanline_IRQ
                let status = &mut self.ppu_status;
                let irq_state = &mut self.irq_state;
                // Wait for three consecutive fetches to match the same address: the two dummy NT
                // fetches that end a scanline plus the next scanline's first NT fetch, which reads
                // the same address because v has not moved in between.
                if addr <= 0x2FFF && Some(addr) == irq_state.prev_addr {
                    irq_state.match_count = irq_state.match_count.saturating_add(1);
                    if irq_state.match_count >= 2 {
                        // Detection lands on the new scanline's first fetch, so the column counter
                        // restarts with it.
                        status.tile_number = 0;
                    }
                    if irq_state.match_count == 2 {
                        if irq_state.in_frame {
                            // Scanline IRQ detected
                            status.scanline += 1;
                            if status.scanline == self.regs.irq_scanline {
                                irq_state.pending = true;
                                if self.regs.irq_enabled {
                                    self.regs.irq_pending = true;
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

                if self.split_active() && is_nt_fetch {
                    self.update_split_region();
                }
                self.split_nametable_read(memory, is_nt_fetch)
                    .or_else(|| self.nametable_read(memory, addr))
            }
            _ => None,
        }
    }

    fn chr_peek(&self, memory: &Memory, addr: u16) -> Option<u8> {
        match addr {
            0x0000..=0x1FFF => self
                .split_chr_read(memory, addr)
                .or_else(|| self.ex_attr_chr_read(memory, addr)),
            0x2000..=0x3EFF => {
                let is_nt_fetch = addr <= 0x2FFF && !ppu::is_attr(addr);
                self.split_nametable_read(memory, is_nt_fetch)
                    .or_else(|| self.nametable_read(memory, addr))
            }
            _ => None,
        }
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr {
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
            0x5011 if self.dmc_mode == 0 && val != 0x00 => {
                // [DDDD DDDD] PCM Data
                // Write mode - writing $00 has no effect
                self.dmc.write_output(val);
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
                self.update_prg_banks(memory);
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
                self.select_chr_set(memory, true);
            }
            0x5102 | 0x5103 => {
                // To allow writing to PRG-RAM you must set:
                //    A=%10
                //    B=%01
                // Any other value will prevent PRG-RAM writing.
                // [.... ..AA]    PRG-RAM Protect A
                // [.... ..BB]    PRG-RAM Protect B
                self.regs.prg_ram_protect[(addr - 0x5102) as usize] = val & 0x03;
                self.update_prg_banks(memory);
            }
            0x5104 => {
                // [.... ..XX] ExRam mode
                //   Value  RAM $5C00-$5FFF  RAM Nametable  Extended Attr
                //   %00    Write Only       Yes            No
                //   %01    Write Only       Yes            Yes
                //   %10    Read/Write       No             No
                //   %11    Read Only        No             No
                self.regs.exram_mode.set(val);
                self.update_ex_ram(memory);
                self.update_nametables(memory);
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
                //
                // Only reported, never applied: the four slots are mapped individually below,
                // since two of the four sources are not CIRAM.
                self.mirroring = match val {
                    0x50 => Mirroring::Horizontal,
                    0x44 => Mirroring::Vertical,
                    0x00 => Mirroring::SingleScreenA,
                    0x55 => Mirroring::SingleScreenB,
                    // Any other combination means Mapper provides nametables
                    _ => Mirroring::FourScreen,
                };
                self.update_nametables(memory);
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
                self.update_prg_banks(memory);
            }
            0x5120..=0x512B => {
                let bank = (addr - 0x5120) as usize;
                self.regs.chr_banks[bank] = val as usize;
                if addr < 0x5128 {
                    self.last_chr_write = ChrBank::Spr;
                } else {
                    // Mirroring BG
                    self.regs.chr_banks[bank + 4] = self.regs.chr_banks[bank];
                    self.last_chr_write = ChrBank::Bg;
                }
                self.select_chr_set(memory, true);
            }
            0x5130 => {
                // [.... ..HH]  CHR Bank Hi bits
                self.regs.chr_hi = (val as usize & 0x03) << 8;
                self.update_chr_banks(memory);
            }
            0x5200 => {
                // [ES.T TTTT]    Split control
                //   E = Enable  (0=split mode disabled, 1=split mode enabled)
                //   S = Vsplit side  (0=split will be on left side, 1=split will be on right)
                //   T = tile number to split at
                self.regs.vsplit.mode = val;
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
                    self.regs.irq_pending = false;
                } else if self.irq_state.pending {
                    self.regs.irq_pending = true;
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
            0x5207..=0x5209 => (),
            // Modes 2 and 3 are served by the page mapping, which already stored or discarded the
            // byte. Modes 0 and 1 leave the window unmapped, so the store happens here - and only
            // latches the value while the PPU is rendering.
            0x5C00..=0x5FFF if !self.regs.exram_mode.rw.contains(ExRamRW::R) => {
                let val = if self.ppu_status.rendering { val } else { 0x00 };
                memory.region_write(Src::ExRam, (addr & 0x03FF) as usize, val);
            }
            // PRG-RAM stores already happened in `Bus`, gated by the page's writable flag.
            _ => (),
        }
    }

    /// Synchronize a write to a PPU register at a given address.
    fn ppu_write(&mut self, addr: u16, val: u8) {
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

    fn update_banks(&mut self, memory: &mut Memory) {
        self.update_prg_banks(memory);
        self.select_chr_set(memory, true);
        self.update_nametables(memory);
        self.update_ex_ram(memory);
    }

    fn clock(&mut self) {
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
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.regs.prg_mode = PrgMode::Bank8k;
        self.regs.chr_mode = ChrMode::Bank1k;
    }

    fn region(&self) -> NesRegion {
        self.region
    }

    /// Both of MMC5's region-dependent clocks, which are separate and were easy to miss.
    ///
    /// `self.region` drives the half-frame pulse timer in [`Map::clock`] via
    /// `Cpu::region_clock_rate`; `self.dmc` has its own rate table like the APU's. Updating only
    /// the DMC left the pulse timer running at the region the cart was *constructed* with.
    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.dmc.set_region(region);
    }

    fn output(&self) -> f32 {
        // Runs once per CPU cycle, so index the mixer tables straight from the integer channel
        // levels. Going through each channel's `output` would convert its level to a float only
        // to convert it back with a saturating float-to-int cast.
        let pulse = PULSE_TABLE[usize::from(self.pulse1.level() + self.pulse2.level())];
        let dmc = TND_TABLE[usize::from(self.dmc.level())];
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
            .field("tile_cache", &self.tile_cache)
            .field("chr_set", &self.chr_set)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MemoryLayout, PAGE_SIZE};

    /// 128K PRG-ROM, 64K PRG-RAM, 32K CHR-ROM, with every 1K page of ROM filled with its own
    /// index so that a read identifies the bank it came from.
    fn test_cart() -> Cart {
        let mut cart = Cart::empty_sized(128 * 1024, 32 * 1024);
        cart.memory = Memory::new(MemoryLayout {
            prg_rom: 128 * 1024,
            prg_ram: 64 * 1024,
            chr: 32 * 1024,
            chr_writable: false,
            ciram: 2 * 1024,
            ex_ram: 1024,
            ..Default::default()
        });
        for (i, page) in cart
            .memory
            .region_mut(Src::PrgRom)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            page.fill(i as u8);
        }
        for (i, page) in cart
            .memory
            .region_mut(Src::Chr)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            page.fill(0x80 | i as u8);
        }
        cart
    }

    fn load() -> (Mapper, Cart) {
        let mut cart = test_cart();
        let mapper = Exrom::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    fn exrom(mapper: &mut Mapper) -> &mut Exrom {
        match mapper {
            Mapper::Exrom(exrom) => exrom,
            _ => unreachable!("mapper is an Exrom"),
        }
    }

    /// Mirrors `Bus::write`: the data store happens first, then the board acts on the register.
    fn write(mapper: &mut Mapper, cart: &mut Cart, addr: u16, val: u8) {
        cart.memory.prg_write(addr, val);
        mapper.write_register(&mut cart.memory, addr, val);
    }

    /// Mirrors `Bus::peek`'s routing for a page-table board.
    fn prg_peek(mapper: &Mapper, cart: &Cart, addr: u16) -> u8 {
        mapper
            .prg_peek(addr)
            .unwrap_or_else(|| cart.memory.prg_peek(addr))
    }

    /// Mirrors `Bus::chr_peek`'s routing for a page-table board.
    fn chr_peek(mapper: &Mapper, cart: &Cart, addr: u16) -> u8 {
        mapper
            .chr_peek(&cart.memory, addr)
            .unwrap_or_else(|| cart.memory.chr_peek(addr))
    }

    #[test]
    fn powers_on_with_the_last_prg_bank_fixed_at_e000() {
        let (mapper, cart) = load();
        // 128K of PRG-ROM in 8K banks is 16 banks; the last starts at 1K page 120.
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120);
        // $5114-$5116 power on with the ROM-select bit clear, so the rest of the space is RAM.
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
    }

    #[test]
    fn prg_mode_8k_maps_each_register_independently() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5100, 3); // 8K mode
        write(&mut mapper, &mut cart, 0x5114, 0x80 | 2);
        write(&mut mapper, &mut cart, 0x5115, 0x80 | 3);
        write(&mut mapper, &mut cart, 0x5116, 0x80 | 4);
        // $5117 has no ROM-select bit; it is always ROM.
        write(&mut mapper, &mut cart, 0x5117, 5);

        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 4 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 5 * 8);
    }

    #[test]
    fn prg_mode_32k_ignores_the_low_two_bank_bits() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5100, 0); // 32K mode
        // Bank 11 aligns down to 8, covering banks 8-11.
        write(&mut mapper, &mut cart, 0x5117, 11);

        for (i, addr) in [0x8000, 0xA000, 0xC000, 0xE000].into_iter().enumerate() {
            assert_eq!(prg_peek(&mapper, &cart, addr), ((8 + i) * 8) as u8);
        }
    }

    #[test]
    fn prg_mode_16k_ignores_the_low_bank_bit() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5100, 1); // 16K mode
        write(&mut mapper, &mut cart, 0x5115, 0x80 | 5); // aligns down to banks 4-5
        write(&mut mapper, &mut cart, 0x5117, 11); // aligns down to banks 10-11

        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 4 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 5 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 10 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 11 * 8);
    }

    /// PRG-RAM banks are selectable in the `$8000-$DFFF` windows too, not just at `$6000`.
    #[test]
    fn prg_ram_banks_are_write_protected_until_5102_and_5103_agree() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5100, 3); // 8K mode
        write(&mut mapper, &mut cart, 0x5114, 1); // RAM bank 1 at $8000

        write(&mut mapper, &mut cart, 0x8000, 0xAA);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x8000),
            0,
            "PRG-RAM writes are discarded until unlocked"
        );

        write(&mut mapper, &mut cart, 0x5102, 0x02);
        write(&mut mapper, &mut cart, 0x5103, 0x01);
        write(&mut mapper, &mut cart, 0x8000, 0xAA);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0xAA);

        // The same RAM bank seen through $6000 - and a different one that must not alias it.
        write(&mut mapper, &mut cart, 0x5113, 1);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0xAA);
        write(&mut mapper, &mut cart, 0x5113, 0);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x00);
    }

    #[test]
    fn chr_1k_mode_maps_all_eight_registers_of_the_written_set() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5101, 3); // 1K banks
        mapper.ppu_write(0x2000, 0x20); // 8x16 sprites, so the 'B' set exists at all
        exrom(&mut mapper).irq_state.in_frame = false;
        for i in 0..8u16 {
            write(&mut mapper, &mut cart, 0x5120 + i, i as u8);
        }
        for i in 0..8u16 {
            assert_eq!(chr_peek(&mapper, &cart, i * 0x0400), 0x80 | i as u8);
        }

        // The 'B' set only has four registers; they repeat across both pattern tables.
        for i in 0..4u16 {
            write(&mut mapper, &mut cart, 0x5128 + i, 8 + i as u8);
        }
        for i in 0..8u16 {
            let expected = 0x80 | (8 + (i & 0x03)) as u8;
            assert_eq!(chr_peek(&mapper, &cart, i * 0x0400), expected);
        }
    }

    /// Set up a board with distinguishable 'A' and 'B' CHR bank sets, mid-frame.
    fn chr_sets() -> (Mapper, Cart) {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5101, 3); // 1K banks
        write(&mut mapper, &mut cart, 0x5120, 1); // 'A' set
        write(&mut mapper, &mut cart, 0x5128, 2); // 'B' set, written last
        exrom(&mut mapper).irq_state.in_frame = true;
        (mapper, cart)
    }

    /// One pattern fetch at the given point in the scanline, which is what re-evaluates the set.
    fn pattern_fetch(mapper: &mut Mapper, cart: &mut Cart, tile_number: u32) {
        exrom(mapper).ppu_status.tile_number = tile_number;
        mapper.chr_read(&mut cart.memory, 0x0000);
    }

    /// With 8x16 sprites the pattern fetches for sprites come from the 'A' set and those for the
    /// background from 'B'. The eight garbage nametable fetches at tiles 32-39 of a scanline mark
    /// where the PPU is fetching sprites.
    #[test]
    fn sprite_and_background_bank_sets_swap_partway_through_a_scanline() {
        let (mut mapper, mut cart) = chr_sets();
        mapper.ppu_write(0x2000, 0x20); // 8x16 sprites

        pattern_fetch(&mut mapper, &mut cart, 0);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 2, "background");

        pattern_fetch(&mut mapper, &mut cart, Exrom::SPR_TILE_START);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 1, "sprites");

        pattern_fetch(&mut mapper, &mut cart, Exrom::SPR_TILE_END);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0000),
            0x80 | 2,
            "background again"
        );
    }

    /// "When using 8x8 sprites, only registers $5120-$5127 are used. Registers $5128-$512B are
    /// completely ignored." Selecting the 'B' set by writing one of them must not stick.
    #[test]
    fn eight_by_eight_sprites_ignore_the_background_bank_set() {
        let (mut mapper, mut cart) = chr_sets();
        mapper.ppu_write(0x2000, 0x00); // 8x8 sprites

        for tile_number in [0, Exrom::SPR_TILE_START, Exrom::SPR_TILE_END] {
            pattern_fetch(&mut mapper, &mut cart, tile_number);
            assert_eq!(
                chr_peek(&mapper, &cart, 0x0000),
                0x80 | 1,
                "tile {tile_number}"
            );
        }
    }

    /// Outside a rendered frame there is no scanline to key on, so the set follows whichever
    /// register was written last.
    #[test]
    fn outside_a_frame_the_last_written_bank_set_wins() {
        let (mut mapper, mut cart) = chr_sets();
        mapper.ppu_write(0x2000, 0x20); // 8x16 sprites
        exrom(&mut mapper).irq_state.in_frame = false;

        write(&mut mapper, &mut cart, 0x5120, 1);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 1);
        write(&mut mapper, &mut cart, 0x5128, 2);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 2);
    }

    #[test]
    fn each_nametable_slot_selects_its_own_source() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5104, 0); // ExRAM usable as a nametable
        // [DDCC BBAA]: A=ScreenA, B=ScreenB, C=ExRAM, D=Fill
        write(&mut mapper, &mut cart, 0x5105, 0b11_10_01_00);
        write(&mut mapper, &mut cart, 0x5106, 0x5A); // fill tile
        write(&mut mapper, &mut cart, 0x5107, 0x02); // fill attribute

        cart.memory.chr_write(0x2000, 0x11);
        cart.memory.chr_write(0x2400, 0x22);
        cart.memory.chr_write(0x2800, 0x33);
        assert_eq!(chr_peek(&mapper, &cart, 0x2000), 0x11);
        assert_eq!(chr_peek(&mapper, &cart, 0x2400), 0x22);
        assert_eq!(chr_peek(&mapper, &cart, 0x2800), 0x33);
        assert_eq!(
            cart.memory.region_ref(Src::ExRam)[0],
            0x33,
            "the third slot must be ExRAM, not CIRAM"
        );

        // Fill mode is synthesised rather than stored, so it ignores writes.
        cart.memory.chr_write(0x2C00, 0x99);
        assert_eq!(chr_peek(&mapper, &cart, 0x2C00), 0x5A);
        assert_eq!(chr_peek(&mapper, &cart, 0x2C00 | 0x03C0), 0xAA);

        // $3000-$3EFF mirrors $2000-$2EFF.
        assert_eq!(chr_peek(&mapper, &cart, 0x3000), 0x11);
        assert_eq!(chr_peek(&mapper, &cart, 0x3400), 0x22);
    }

    /// ExRAM is not a nametable outside modes 0 and 1, and reads back as zero when selected.
    #[test]
    fn exram_nametables_read_zero_outside_nametable_modes() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5104, 0);
        write(&mut mapper, &mut cart, 0x5105, 0b10_10_10_10);
        cart.memory.chr_write(0x2000, 0x77);
        assert_eq!(chr_peek(&mapper, &cart, 0x2000), 0x77);

        write(&mut mapper, &mut cart, 0x5104, 2);
        assert_eq!(chr_peek(&mapper, &cart, 0x2000), 0x00);
    }

    #[test]
    fn exram_window_follows_its_access_mode() {
        let (mut mapper, mut cart) = load();

        // Mode 2: read/write.
        write(&mut mapper, &mut cart, 0x5104, 2);
        write(&mut mapper, &mut cart, 0x5C00, 0x37);
        assert_eq!(prg_peek(&mapper, &cart, 0x5C00), 0x37);

        // Mode 3: read-only.
        write(&mut mapper, &mut cart, 0x5104, 3);
        write(&mut mapper, &mut cart, 0x5C00, 0x99);
        assert_eq!(prg_peek(&mapper, &cart, 0x5C00), 0x37);

        // Modes 0/1: write-only, and the byte only latches while the PPU is rendering.
        write(&mut mapper, &mut cart, 0x5104, 0);
        assert_eq!(prg_peek(&mapper, &cart, 0x5C00), 0x00, "not readable");
        write(&mut mapper, &mut cart, 0x5C00, 0x42);
        mapper.ppu_write(0x2001, 0x18); // enable rendering
        write(&mut mapper, &mut cart, 0x5C01, 0x42);

        write(&mut mapper, &mut cart, 0x5104, 2);
        assert_eq!(prg_peek(&mapper, &cart, 0x5C00), 0x00, "written while idle");
        assert_eq!(
            prg_peek(&mapper, &cart, 0x5C01),
            0x42,
            "written while rendering"
        );
    }

    /// In extended-attribute mode a byte of ExRAM per nametable entry supplies both the palette
    /// and a 4K CHR bank, so neither the attribute fetch nor the pattern fetch can be a page.
    #[test]
    fn extended_attribute_mode_sources_the_bank_and_palette_from_exram() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5104, 1);
        // Palette 1 in the top two bits, CHR bank 1 in the bottom six.
        cart.memory.region_write(Src::ExRam, 5, 0x41);

        // The nametable fetch caches which ExRAM byte the following fetches use.
        mapper.chr_read(&mut cart.memory, 0x2005);

        assert_eq!(
            chr_peek(&mapper, &cart, 0x23C0),
            0x55,
            "palette 1, mirrored"
        );
        // 4K bank 1 starts at CHR 1K page 4.
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 4);
        assert_eq!(chr_peek(&mapper, &cart, 0x0400), 0x80 | 5);
    }

    /// The split covers the columns on one side of the delimiter written to $5200, which the board
    /// tracks by counting nametable fetches rather than by any address the PPU puts on the bus.
    #[test]
    fn vertical_split_covers_the_columns_on_the_selected_side() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5104, 0); // ExRAM nametable mode
        write(&mut mapper, &mut cart, 0x5200, 0x80 | 0x40 | 20); // enabled, right side, column 20

        let exrom = exrom(&mut mapper);
        exrom.irq_state.in_frame = true;
        // Chronological order: the last two fetches of a scanline prefetch columns 0 and 1 of the
        // next one, so they come first.
        for tile_number in (40..42).chain(0..40) {
            exrom.ppu_status.tile_number = tile_number;
            exrom.update_split_region();
            let column = exrom.split_column();
            // Columns 20 onwards are the split; 33 and up are sprite-fetch garbage, not columns.
            assert_eq!(
                exrom.regs.vsplit.in_region,
                (20..=32).contains(&column),
                "column {column} (tile {tile_number})"
            );
        }
    }

    /// Inside the split, nametable and attribute bytes come from ExRAM laid out as a nametable of
    /// its own, and patterns from the 4K bank in $5202 - none of which the page tables can express.
    #[test]
    fn vertical_split_reads_its_own_nametable_attributes_and_patterns() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5104, 0); // ExRAM nametable mode
        write(&mut mapper, &mut cart, 0x5200, 0x80 | 20); // enabled, left side, column 20
        write(&mut mapper, &mut cart, 0x5201, 0); // no split scroll
        write(&mut mapper, &mut cart, 0x5202, 2); // 4K CHR bank 2
        {
            let exrom = exrom(&mut mapper);
            exrom.irq_state.in_frame = true;
            exrom.ppu_status.tile_number = 40; // column 0, where the left side enters the split
            exrom.update_split_region();
            assert!(exrom.regs.vsplit.in_region);
            assert_eq!(exrom.regs.vsplit.tile_addr, 0);
        }

        cart.memory.region_write(Src::ExRam, 0x0000, 0x5C); // the split's tile
        cart.memory.region_write(Src::ExRam, 0x03C0, 0b01); // and its attribute
        assert_eq!(mapper.chr_peek(&cart.memory, 0x2000), Some(0x5C));
        assert_eq!(mapper.chr_peek(&cart.memory, 0x23C0), Some(0x55));
        // 4K bank 2 of the test cart's CHR starts at 1K page 8.
        assert_eq!(mapper.chr_peek(&cart.memory, 0x0000), Some(0x80 | 8));

        // Outside the split, everything falls back to the ordinary sources.
        exrom(&mut mapper).regs.vsplit.in_region = false;
        assert_eq!(mapper.chr_peek(&cart.memory, 0x0000), None);
    }

    /// The hardware raises the pending flag whenever the scanline matches, and gates only the
    /// assertion to the CPU on the enable bit (`docs/mapper/005.txt:411`). A game that polls $5204
    /// with IRQs disabled has to see it.
    #[test]
    fn the_pending_flag_reads_back_even_with_irqs_disabled() {
        let (mut mapper, cart) = load();
        {
            let exrom = exrom(&mut mapper);
            exrom.irq_state.in_frame = true;
            // What a scanline match does: raise the hardware flag, and assert to the CPU only if
            // enabled - which it is not.
            exrom.irq_state.pending = true;
            exrom.regs.irq_enabled = false;
            exrom.regs.irq_pending = false;
        }

        assert_eq!(
            prg_peek(&mapper, &cart, 0x5204) & 0x80,
            0x80,
            "the pending flag reads back set"
        );
        assert!(
            !mapper.irq_pending(),
            "while the CPU is still not being interrupted"
        );
    }

    /// The last two nametable fetches of a scanline prefetch columns 0 and 1 of the *next* one,
    /// and the scanline counter does not move until the dummy fetches after them - so both have to
    /// scroll a line further, not just the second.
    #[test]
    fn both_nametable_prefetches_scroll_to_the_next_scanline() {
        let (mut mapper, _cart) = load();
        let exrom = exrom(&mut mapper);
        exrom.ppu_status.scanline = 5;
        exrom.regs.vsplit.scroll = 0;

        exrom.ppu_status.tile_number = 39;
        assert_eq!(exrom.split_scroll(), 5, "still the current scanline");
        for tile_number in [40, 41] {
            exrom.ppu_status.tile_number = tile_number;
            assert_eq!(
                exrom.split_scroll(),
                6,
                "tile {tile_number} is a prefetch for the next scanline"
            );
        }
    }

    /// Page tables are derived state that save states do not carry, so `update_banks` has to
    /// rebuild every one of MMC5's mappings from its registers alone - which is what
    /// [`Bus::rebuild_mapper_state`](crate::bus::Bus::rebuild_mapper_state) relies on.
    #[test]
    fn update_banks_rebuilds_every_mapping_from_register_state() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5100, 3);
        write(&mut mapper, &mut cart, 0x5114, 0x80 | 2);
        write(&mut mapper, &mut cart, 0x5101, 3);
        write(&mut mapper, &mut cart, 0x5120, 6);
        write(&mut mapper, &mut cart, 0x5104, 2);
        write(&mut mapper, &mut cart, 0x5105, 0b01_01_00_00);
        write(&mut mapper, &mut cart, 0x5C00, 0x37);
        cart.memory.chr_write(0x2800, 0x64);

        let config = bincode::config::legacy();
        let bytes = bincode::serde::encode_to_vec(&cart.memory, config).expect("memory serializes");
        let (mut restored, _) = bincode::serde::decode_from_slice::<Memory, _>(&bytes, config)
            .expect("memory deserializes");
        // Save states carry only the mutable tail, so ROM comes back from the running console -
        // `Bus::load_state` does this for real.
        assert!(restored.restore_rom_from(&cart.memory), "same cart");
        cart.memory = restored;
        mapper.update_banks(&mut cart.memory);

        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 8, "PRG");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 6, "CHR");
        assert_eq!(prg_peek(&mapper, &cart, 0x5C00), 0x37, "ExRAM window");
        assert_eq!(chr_peek(&mapper, &cart, 0x2800), 0x64, "nametables");
    }
}
