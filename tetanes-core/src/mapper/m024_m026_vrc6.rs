//! `VRC6` (Mapper 024).
//!
//! <https://www.nesdev.org/wiki/VRC6>

use crate::{
    apu::PULSE_TABLE,
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sample, Sram},
    mapper::{self, Map, Mapper, vrc_irq::VrcIrq},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `VRC6` revision.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    /// VRC6a
    #[default]
    A,
    /// VRC6b
    B,
}

/// `VRC6` registers.
#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    pub banking_mode: u8,
    pub prg: [usize; 4],
    pub chr: [usize; 8],
}

/// `VRC6` (Mapper 024).
/// `VRC6` (Mapper 024).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc6 {
    pub regs: Regs,
    pub revision: Revision,
    pub mirroring: Mirroring,
    pub irq: VrcIrq,
    pub audio: Audio,
    /// Page selected by each of the four nametable slots, indexing CIRAM or CHR-ROM depending on
    /// bit 4 of the banking mode.
    pub nt_banks: [usize; 4],
    /// 16K bank at $8000 and 8K bank at $C000. $E000 is fixed to the last bank.
    pub prg_banks: [usize; 2],
}

impl Vrc6 {
    const PRG_WINDOW: usize = 8 * 1024;
    const PRG_WINDOW_16K: usize = 16 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;

    // PPU $0000..=$1FFF 8x 1K CHR Banks, grouped by banking mode
    // PPU $2000..=$3FFF 4x 1K Nametables, from CIRAM or from CHR-ROM
    // CPU $6000..=$7FFF 8K PRG-RAM, enabled by bit 7 of the banking mode
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable
    // CPU $C000..=$DFFF 8K PRG-ROM Bank Switchable
    // CPU $E000..=$FFFF 8K PRG-ROM Fixed to Last Bank
    pub fn load(cart: &mut Cart, revision: Revision) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            regs: Regs::default(),
            revision,
            mirroring: cart.mirroring(),
            irq: VrcIrq::default(),
            audio: Audio::new(),
            nt_banks: [0; 4],
            prg_banks: [0; 2],
        };
        board.set_mirroring(board.mirroring);
        board.sync(&mut cart.memory);
        Ok(board.into())
    }

    #[inline(always)]
    #[must_use]
    pub const fn prg_ram_enabled(&self) -> bool {
        self.regs.banking_mode & 0x80 == 0x80
    }

    pub const fn set_nametable_page(&mut self, bank: usize, page: usize) {
        self.nt_banks[bank] = page;
    }

    pub const fn set_nametables(&mut self, nametables: &[usize; 4]) {
        self.nt_banks = *nametables;
    }

    /// Translate a mirroring mode into the four nametable page selections.
    pub const fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
        self.nt_banks = match mirroring {
            Mirroring::Vertical => [0, 1, 0, 1],
            Mirroring::Horizontal => [0, 0, 1, 1],
            Mirroring::SingleScreenA => [0; 4],
            Mirroring::SingleScreenB => [1; 4],
            Mirroring::FourScreen => [0, 1, 2, 3],
        };
    }

    /// Compute the eight 1K CHR bank selections for the current banking mode.
    const fn chr_banks(&self) -> [usize; 8] {
        let chr = &self.regs.chr;
        // Bit 5 forces bank pairs to be aligned, ignoring the low bit.
        let (mask, or_mask) = if self.regs.banking_mode & 0x20 == 0x20 {
            (0xFE, 1)
        } else {
            (0xFF, 0)
        };
        match self.regs.banking_mode & 0x03 {
            // Eight independent 1K banks.
            0 => [
                chr[0], chr[1], chr[2], chr[3], chr[4], chr[5], chr[6], chr[7],
            ],
            // Four 2K banks.
            1 => [
                chr[0] & mask,
                (chr[0] & mask) | or_mask,
                chr[1] & mask,
                (chr[1] & mask) | or_mask,
                chr[2] & mask,
                (chr[2] & mask) | or_mask,
                chr[3] & mask,
                (chr[3] & mask) | or_mask,
            ],
            // Four 1K banks then two 2K banks.
            _ => [
                chr[0],
                chr[1],
                chr[2],
                chr[3],
                chr[4] & mask,
                (chr[4] & mask) | or_mask,
                chr[5] & mask,
                (chr[5] & mask) | or_mask,
            ],
        }
    }

    /// Recompute the nametable page selections from the banking mode.
    const fn update_nametables(&mut self) {
        let chr = self.regs.chr;
        if self.regs.banking_mode & 0x10 == 0x10 {
            // Nametables come from CHR-ROM, so every slot is independently selectable.
            self.mirroring = Mirroring::FourScreen;
            self.nt_banks = match self.regs.banking_mode & 0x2F {
                0x20 | 0x27 => [
                    chr[6] & 0xFE,
                    (chr[6] & 0xFE) | 1,
                    chr[7] & 0xFE,
                    (chr[7] & 0xFE) | 1,
                ],
                0x23 | 0x24 => [
                    chr[6] & 0xFE,
                    chr[7] & 0xFE,
                    (chr[6] & 0xFE) | 1,
                    (chr[7] & 0xFE) | 1,
                ],
                0x28 | 0x2F => [chr[6] & 0xFE, chr[6] & 0xFE, chr[7] & 0xFE, chr[7] & 0xFE],
                0x2B | 0x2C => [
                    (chr[6] & 0xFE) | 1,
                    (chr[7] & 0xFE) | 1,
                    (chr[6] & 0xFE) | 1,
                    (chr[7] & 0xFE) | 1,
                ],
                _ => match self.regs.banking_mode & 0x07 {
                    0 | 6 | 7 => [chr[6], chr[6], chr[7], chr[7]],
                    1 | 5 => [chr[4], chr[5], chr[6], chr[7]],
                    _ => [chr[6], chr[7], chr[6], chr[7]],
                },
            };
        } else {
            match self.regs.banking_mode & 0x2F {
                0x20 | 0x27 => self.set_mirroring(Mirroring::Vertical),
                0x23 | 0x24 => self.set_mirroring(Mirroring::Horizontal),
                0x28 | 0x2F => self.set_mirroring(Mirroring::SingleScreenA),
                0x2B | 0x2C => self.set_mirroring(Mirroring::SingleScreenB),
                _ => {
                    // Non-standard layouts still pick CIRAM pages per slot.
                    self.mirroring = Mirroring::FourScreen;
                    self.nt_banks = match self.regs.banking_mode & 0x07 {
                        0 | 6 | 7 => [chr[6] & 0x01, chr[6] & 0x01, chr[7] & 0x01, chr[7] & 0x01],
                        1 | 5 => [chr[4] & 0x01, chr[5] & 0x01, chr[6] & 0x01, chr[7] & 0x01],
                        _ => [chr[6] & 0x01, chr[7] & 0x01, chr[6] & 0x01, chr[7] & 0x01],
                    };
                }
            }
        }
    }
}

impl Map for Vrc6 {
    fn uses_page_tables(&self) -> bool {
        true
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq.irq_pending
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        // VRC6b swaps the low two address lines.
        let addr = if self.revision == Revision::B {
            (addr & 0xFFFC) | ((addr & 0x01) << 1) | ((addr & 0x02) >> 1)
        } else {
            addr
        };
        match addr & 0xF003 {
            0x8000..=0x8003 => self.prg_banks[0] = usize::from(val & 0x0F),
            0x9000..=0x9003 | 0xA000..=0xA002 | 0xB000..=0xB002 => {
                self.audio.write_register(addr, val);
                return;
            }
            0xB003 => {
                self.regs.banking_mode = val;
                self.update_nametables();
            }
            0xC000..=0xC003 => self.prg_banks[1] = usize::from(val & 0x1F),
            0xD000..=0xD003 => {
                self.regs.chr[(addr & 0x03) as usize] = val.into();
                self.update_nametables();
            }
            0xE000..=0xE003 => {
                self.regs.chr[(4 + (addr & 0x03)) as usize] = val.into();
                self.update_nametables();
            }
            0xF000 => self.irq.write_reload(val),
            0xF001 => self.irq.write_control(val),
            0xF002 => self.irq.acknowledge(),
            _ => return,
        }
        self.sync(memory);
    }

    fn sync(&mut self, memory: &mut Memory) {
        for (slot, bank) in self.chr_banks().into_iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, bank as i32, Src::Chr);
        }

        // Nametables are page entries like any other, so CHR-ROM-as-nametable stops needing a
        // special case in the read path.
        let src = if self.regs.banking_mode & 0x10 == 0x10 {
            Src::Chr
        } else {
            Src::CiRam
        };
        for (slot, bank) in self.nt_banks.into_iter().enumerate() {
            let addr = 0x2000 + (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, bank as i32, src);
            // $3000-$3FFF mirrors $2000-$2FFF.
            memory.map_chr(addr + 0x1000, Self::CHR_WINDOW, bank as i32, src);
        }

        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW_16K,
            self.prg_banks[0] as i32,
            Src::PrgRom,
        );
        memory.map_prg(
            0xC000,
            Self::PRG_WINDOW,
            self.prg_banks[1] as i32,
            Src::PrgRom,
        );
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);

        if self.prg_ram_enabled() {
            memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        } else {
            memory.unmap_prg(0x6000, Self::PRG_RAM_WINDOW);
        }
    }
}

impl Reset for Vrc6 {
    fn reset(&mut self, kind: ResetKind) {
        self.irq.reset(kind);
        self.audio.reset(kind);
    }
}

impl Clock for Vrc6 {
    fn clock(&mut self) {
        self.irq.clock();
        self.audio.clock();
    }
}

impl Regional for Vrc6 {}
impl Sram for Vrc6 {}

impl Sample for Vrc6 {
    fn output(&self) -> f32 {
        self.audio.output()
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Audio {
    pub pulse1: Pulse,
    pub pulse2: Pulse,
    pub saw: Saw,
    pub halt: bool,
    pub out: f32,
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    const fn new() -> Self {
        Self {
            pulse1: Pulse::new(),
            pulse2: Pulse::new(),
            saw: Saw::new(),
            halt: false,
            out: 0.0,
        }
    }

    #[must_use]
    fn output(&self) -> f32 {
        let pulse_scale = PULSE_TABLE[PULSE_TABLE.len() - 1] / 15.0;
        pulse_scale * self.out
    }

    fn write_register(&mut self, addr: u16, val: u8) {
        // Only A0, A1 and A12-15 are used for registers, remaining addresses are mirrored.
        match addr & 0xF003 {
            0x9000..=0x9002 => self.pulse1.write_register(addr, val),
            0x9003 => {
                self.halt = val & 0x01 == 0x01;
                let freq_shift = if val & 0x04 == 0x04 {
                    8
                } else if val & 0x02 == 0x02 {
                    4
                } else {
                    0
                };
                self.pulse1.set_freq_shift(freq_shift);
                self.pulse2.set_freq_shift(freq_shift);
                self.saw.set_freq_shift(freq_shift);
            }
            0xA000..=0xA002 => self.pulse2.write_register(addr, val),
            0xB000..=0xB002 => self.saw.write_register(addr, val),
            _ => unreachable!("impossible Vrc6Audio register: {}", addr),
        }
    }
}

impl Clock for Audio {
    fn clock(&mut self) {
        if !self.halt {
            self.pulse1.clock();
            self.pulse2.clock();
            self.saw.clock();

            self.out = self.pulse1.volume() + self.pulse2.volume() + self.saw.volume();
        }
    }
}

impl Reset for Audio {
    fn reset(&mut self, _kind: ResetKind) {
        self.halt = false;
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Pulse {
    pub enabled: bool,
    pub volume: u8,
    pub duty_cycle: u8,
    pub ignore_duty: bool,
    pub frequency: u16,
    pub timer: u16,
    pub step: u8,
    pub freq_shift: u8,
}

impl Default for Pulse {
    fn default() -> Self {
        Self::new()
    }
}

impl Pulse {
    const fn new() -> Self {
        Self {
            enabled: false,
            volume: 0,
            duty_cycle: 0,
            ignore_duty: false,
            frequency: 1,
            timer: 1,
            step: 0,
            freq_shift: 0,
        }
    }

    fn write_register(&mut self, addr: u16, val: u8) {
        match addr & 0x03 {
            0 => {
                self.volume = val & 0x0F;
                self.duty_cycle = (val & 0x70) >> 4;
                self.ignore_duty = val & 0x80 == 0x80;
            }
            1 => self.frequency = (self.frequency & 0x0F00) | u16::from(val),
            2 => {
                self.frequency = ((u16::from(val) & 0x0F) << 8) | (self.frequency & 0xFF);
                self.enabled = val & 0x80 == 0x80;
                if !self.enabled {
                    self.step = 0;
                }
            }
            _ => unreachable!("impossible Vrc6Pulse register: {}", addr),
        }
    }

    const fn set_freq_shift(&mut self, val: u8) {
        self.freq_shift = val;
    }

    fn volume(&self) -> f32 {
        if self.enabled && (self.ignore_duty || self.step <= self.duty_cycle) {
            f32::from(self.volume)
        } else {
            0.0
        }
    }
}

impl Clock for Pulse {
    fn clock(&mut self) {
        if self.enabled {
            self.timer -= 1;
            if self.timer == 0 {
                self.step = (self.step + 1) & 0x0F;
                self.timer = (self.frequency >> self.freq_shift) + 1;
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Saw {
    pub enabled: bool,
    pub accum: u8,
    pub accum_rate: u8,
    pub frequency: u16,
    pub timer: u16,
    pub step: u8,
    pub freq_shift: u8,
}

impl Default for Saw {
    fn default() -> Self {
        Self::new()
    }
}

impl Saw {
    const fn new() -> Self {
        Self {
            enabled: false,
            accum: 0,
            accum_rate: 0,
            frequency: 1,
            timer: 1,
            step: 0,
            freq_shift: 0,
        }
    }

    fn write_register(&mut self, addr: u16, val: u8) {
        match addr & 0x03 {
            0 => {
                self.accum_rate = val & 0x3F;
            }
            1 => self.frequency = (self.frequency & 0x0F00) | u16::from(val),
            2 => {
                self.frequency = ((u16::from(val) & 0x0F) << 8) | (self.frequency & 0xFF);
                self.enabled = val & 0x80 == 0x80;
                if !self.enabled {
                    self.accum = 0;
                    self.step = 0;
                }
            }
            _ => unreachable!("impossible Vrc6Saw register: {}", addr),
        }
    }

    const fn set_freq_shift(&mut self, val: u8) {
        self.freq_shift = val;
    }

    fn volume(&self) -> f32 {
        if self.enabled {
            f32::from(self.accum >> 3)
        } else {
            0.0
        }
    }
}

impl Clock for Saw {
    fn clock(&mut self) {
        if self.enabled {
            self.timer -= 1;
            if self.timer == 0 {
                self.step = (self.step + 1) % 14;
                self.timer = (self.frequency >> self.freq_shift) + 1;

                if self.step == 0 {
                    self.accum = 0;
                } else if self.step & 0x01 == 0x00 {
                    self.accum += self.accum_rate;
                }
            }
        }
    }
}
