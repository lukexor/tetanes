//! `Sunsoft FME7` (Mapper 069).
//!
//! <https://www.nesdev.org/wiki/Sunsoft_FME-7>

use crate::{
    apu::PULSE_TABLE,
    cart::Cart,
    common::{Clock, Regional, Reset, Sample, Sram},
    cpu::{Cpu, Irq},
    mapper::{
        self, MapRead, MapWrite, MappedRead, MappedWrite, Mapper, Mirrored, OnBusRead, OnBusWrite,
    },
    mem::Banks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `Sunsoft FME7` registers.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    command: u8,
    parameter: u8,
    prg_ram_enabled: bool,
    irq_enabled: bool,
    irq_counter_enabled: bool,
    irq_counter: u16,
}

/// `Sunsoft FME7` (Mapper 069).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct SunsoftFme7 {
    pub regs: Regs,
    pub mirroring: Mirroring,
    pub audio: Audio,
    pub chr_banks: Banks,
    pub prg_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl SunsoftFme7 {
    const PRG_WINDOW: usize = 8 * 1024;
    const PRG_RAM_SIZE: usize = 32 * 1024;
    const CHR_WINDOW: usize = 1024;

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        cart.add_prg_ram(Self::PRG_RAM_SIZE);
        let mut sunsoft_fme7 = Self {
            regs: Regs::default(),
            mirroring: cart.mirroring(),
            audio: Audio::new(),
            chr_banks: Banks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_WINDOW)?,
            prg_banks: Banks::new(0x6000, 0x7FFF, cart.prg_ram.len(), Self::PRG_WINDOW)?,
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_WINDOW)?,
        };
        sunsoft_fme7
            .prg_rom_banks
            .set(3, sunsoft_fme7.prg_rom_banks.last());
        Ok(sunsoft_fme7.into())
    }
}

impl Mirrored for SunsoftFme7 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MapRead for SunsoftFme7 {
    // PPU $0000..=$03FF 1K CHR-ROM Bank 1 Switchable
    // PPU $0400..=$07FF 1K CHR-ROM Bank 2 Switchable
    // PPU $0800..=$0BFF 1K CHR-ROM Bank 3 Switchable
    // PPU $0C00..=$0FFF 1K CHR-ROM Bank 4 Switchable
    // PPU $1000..=$13FF 1K CHR-ROM Bank 5 Switchable
    // PPU $1400..=$17FF 1K CHR-ROM Bank 6 Switchable
    // PPU $1800..=$1BFF 1K CHR-ROM Bank 7 Switchable
    // PPU $1C00..=$1FFF 1K CHR-ROM Bank 8 Switchable

    // CPU $6000..=$7FFF 8K PRG-ROM or PRG-RAM Bank 1 Switchable
    // CPU $8000..=$9FFF 8K PRG-ROM Bank 1 Switchable
    // CPU $A000..=$BFFF 8K PRG-ROM Bank 2 Switchable
    // CPU $C000..=$DFFF 8K PRG-ROM Bank 3 Switchable
    // CPU $E000..=$FFFF 8K PRG-ROM Bank 4 fixed to last

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x6000..=0x7FFF => {
                if self.regs.prg_ram_enabled {
                    MappedRead::PrgRam(self.prg_banks.translate(addr))
                } else {
                    MappedRead::PrgRom(self.prg_banks.translate(addr))
                }
            }
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Bus,
        }
    }
}

impl MapWrite for SunsoftFme7 {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x6000..=0x7FFF => {
                if self.regs.prg_ram_enabled {
                    return MappedWrite::PrgRam(self.prg_banks.translate(addr), val);
                }
            }
            0x8000..=0x9FFF => self.regs.command = val & 0x0F,
            0xA000..=0xBFFF => match self.regs.command {
                0..=7 => self.chr_banks.set(self.regs.command.into(), val.into()),
                8 => {
                    self.regs.parameter = val;
                    self.regs.prg_ram_enabled = val & 0x80 == 0x80;
                    self.prg_banks.set(0, (val & 0x3F).into());
                }
                9..=0xB => {
                    let bank = self.regs.command - 9;
                    self.prg_rom_banks.set(bank.into(), (val & 0x3F).into());
                }
                0xC => self.set_mirroring(match val & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenA,
                    _ => Mirroring::SingleScreenB,
                }),
                0xD => {
                    self.regs.irq_enabled = (val & 0x01) == 0x01;
                    self.regs.irq_counter_enabled = (val & 0x80) == 0x80;
                    Cpu::clear_irq(Irq::MAPPER);
                }
                0xE => self.regs.irq_counter = (self.regs.irq_counter & 0xFF00) | u16::from(val),
                0xF => {
                    self.regs.irq_counter = (self.regs.irq_counter & 0xFF) | (u16::from(val) << 8);
                }
                _ => (),
            },
            0xC000..=0xFFFF => self.audio.write_register(addr, val),
            _ => return MappedWrite::Bus,
        }
        MappedWrite::None
    }
}

impl OnBusRead for SunsoftFme7 {}
impl OnBusWrite for SunsoftFme7 {}
impl Reset for SunsoftFme7 {}

impl Clock for SunsoftFme7 {
    fn clock(&mut self) {
        if self.regs.irq_counter_enabled {
            self.regs.irq_counter = self.regs.irq_counter.wrapping_sub(1);
            if self.regs.irq_counter == 0xFFFF && self.regs.irq_enabled {
                Cpu::set_irq(Irq::MAPPER);
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
            0xE000..=0xFFFF => {
                if self.register <= 0x0F {
                    self.registers[self.register as usize] = val;
                }
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
