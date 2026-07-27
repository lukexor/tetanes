//! `Sunsoft FME7` (Mapper 069).
//!
//! <https://www.nesdev.org/wiki/Sunsoft_FME-7>

use crate::{
    apu::PULSE_TABLE,
    cart::Cart,
    common::{Clock, Regional, Reset, Sample, Sram},
    mapper::{self, Map, Mapper, MapperOps},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `Sunsoft FME7` registers.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    pub command: u8,
    pub parameter: u8,
    pub prg_ram_enabled: bool,
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub irq_counter_enabled: bool,
    pub irq_counter: u16,
}

/// `Sunsoft FME7` (Mapper 069).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct SunsoftFme7 {
    pub regs: Regs,
    pub mirroring: Mirroring,
    pub audio: Audio,
    /// 8x 1K CHR banks.
    pub chr_banks: [u8; 8],
    /// 3x 8K PRG-ROM banks at $8000/$A000/$C000. $E000 is fixed to the last bank.
    pub prg_banks: [u8; 3],
    /// Bank selected into the $6000 window, from PRG-RAM or PRG-ROM per `regs.prg_ram_enabled`.
    pub prg_ram_bank: u8,
}

impl SunsoftFme7 {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;

    // PPU $0000..=$1FFF 8x 1K CHR Banks
    // CPU $6000..=$7FFF 8K PRG-RAM or PRG-ROM Bank Switchable
    // CPU $8000..=$DFFF 3x 8K PRG-ROM Banks Switchable
    // CPU $E000..=$FFFF 8K PRG-ROM Fixed to Last Bank
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            regs: Regs::default(),
            mirroring: cart.mirroring(),
            audio: Audio::new(),
            chr_banks: [0; 8],
            prg_banks: [0; 3],
            prg_ram_bank: 0,
        };
        board.sync(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for SunsoftFme7 {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ | MapperOps::AUDIO
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.regs.irq_pending
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr {
            0x8000..=0x9FFF => self.regs.command = val & 0x0F,
            0xA000..=0xBFFF => match self.regs.command {
                0..=7 => self.chr_banks[usize::from(self.regs.command)] = val,
                8 => {
                    self.regs.parameter = val;
                    self.regs.prg_ram_enabled = val & 0x80 == 0x80;
                    self.prg_ram_bank = val & 0x3F;
                }
                9..=0xB => self.prg_banks[usize::from(self.regs.command - 9)] = val & 0x3F,
                0xC => {
                    self.mirroring = match val & 0x03 {
                        0b00 => Mirroring::Vertical,
                        0b01 => Mirroring::Horizontal,
                        0b10 => Mirroring::SingleScreenA,
                        _ => Mirroring::SingleScreenB,
                    }
                }
                0xD => {
                    self.regs.irq_enabled = (val & 0x01) == 0x01;
                    self.regs.irq_counter_enabled = (val & 0x80) == 0x80;
                    self.regs.irq_pending = false;
                }
                0xE => self.regs.irq_counter = (self.regs.irq_counter & 0xFF00) | u16::from(val),
                0xF => {
                    self.regs.irq_counter = (self.regs.irq_counter & 0xFF) | (u16::from(val) << 8);
                }
                _ => (),
            },
            0xC000..=0xFFFF => self.audio.write_register(addr, val),
            _ => return,
        }
        self.sync(memory);
    }

    fn sync(&mut self, memory: &mut Memory) {
        for (slot, bank) in self.chr_banks.iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*bank), Src::Chr);
        }
        // The $6000 window selects the same bank index from either PRG-RAM or PRG-ROM.
        let src = if self.regs.prg_ram_enabled {
            Src::PrgRam
        } else {
            Src::PrgRom
        };
        memory.map_prg(0x6000, Self::PRG_WINDOW, i32::from(self.prg_ram_bank), src);
        for (slot, bank) in self.prg_banks.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, i32::from(*bank), Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.set_mirroring(self.mirroring);
    }
}

impl Reset for SunsoftFme7 {}

impl Clock for SunsoftFme7 {
    fn clock(&mut self) {
        if self.regs.irq_counter_enabled {
            self.regs.irq_counter = self.regs.irq_counter.wrapping_sub(1);
            if self.regs.irq_counter == 0xFFFF && self.regs.irq_enabled {
                self.regs.irq_pending = true;
            }
        }
        self.audio.clock();
    }
}

impl Regional for SunsoftFme7 {}
impl Sram for SunsoftFme7 {}

impl Sample for SunsoftFme7 {
    fn output(&self) -> f32 {
        self.audio.output()
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Audio {
    clock_timer: u8,
    register: u8,
    registers: [u8; 16],
    volumes: [u8; 16],
    timers: [i16; 3],
    steps: [u8; 3],
    out: f32,
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    pub fn new() -> Self {
        let mut audio = Self {
            clock_timer: 1,
            register: 0,
            registers: [0; 16],
            volumes: [0; 16],
            timers: [0; 3],
            steps: [0; 3],
            out: 0.0,
        };
        let mut output = 1.0;
        for volume in audio.volumes.iter_mut().skip(1) {
            // +1.5dB 2x for every 1 step in volume
            output *= 1.188_502_227_437_018_5;
            output *= 1.188_502_227_437_018_5;
            *volume = output as u8;
        }
        audio
    }

    #[must_use]
    #[inline]
    pub fn output(&self) -> f32 {
        let pulse_scale = PULSE_TABLE[PULSE_TABLE.len() - 1] / 15.0;
        pulse_scale * self.out
    }

    #[must_use]
    #[inline]
    pub fn period(&self, channel: usize) -> u16 {
        let register = channel * 2;
        u16::from(self.registers[register]) | (u16::from(self.registers[register + 1]) << 8)
    }

    #[must_use]
    #[inline]
    pub fn envelope_period(&self) -> u16 {
        u16::from(self.registers[0x0B]) | (u16::from(self.registers[0x0C]) << 8)
    }

    #[must_use]
    #[inline]
    pub const fn noise_period(&self) -> u8 {
        self.registers[0x06]
    }

    #[must_use]
    #[inline]
    pub const fn volume(&self, channel: usize) -> u8 {
        self.volumes[(self.registers[channel + 8] & 0x0F) as usize]
    }

    #[must_use]
    #[inline]
    pub const fn envelope_enabled(&self, channel: usize) -> bool {
        self.registers[channel + 8] & 0x10 == 0x10
    }

    #[must_use]
    #[inline]
    pub const fn square_enabled(&self, channel: usize) -> bool {
        (self.registers[0x07] >> channel) & 0x01 == 0x00
    }

    #[must_use]
    #[inline]
    pub const fn noise_enabled(&self, channel: usize) -> bool {
        (self.registers[0x07] >> (channel + 3)) & 0x01 == 0x00
    }

    const fn write_register(&mut self, addr: u16, val: u8) {
        match addr {
            0xC000..=0xDFFF => self.register = val,
            0xE000..=0xFFFF if self.register <= 0x0F => {
                self.registers[self.register as usize] = val;
            }
            _ => (),
        }
    }
}

impl Clock for Audio {
    fn clock(&mut self) {
        if self.clock_timer == 0 {
            self.clock_timer = 1;
            for channel in 0..3 {
                self.timers[channel] -= 1;
                if self.timers[channel] <= 0 {
                    self.timers[channel] = self.period(channel) as i16;
                    self.steps[channel] = (self.steps[channel] + 1) & 0x0F;
                }
            }
            self.out = [0, 1, 2]
                .into_iter()
                .filter(|&channel| self.square_enabled(channel) && self.steps[channel] < 0x08)
                .map(|channel| self.volume(channel) as f32)
                .sum();
        }
        self.clock_timer -= 1;
    }
}
