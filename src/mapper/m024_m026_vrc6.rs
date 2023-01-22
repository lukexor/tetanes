//! `VRC6a` (Mapper 024)
//!
//! <https://www.nesdev.org/wiki/VRC6>

use crate::{
    apu::PULSE_TABLE,
    audio::Audio,
    cart::Cart,
    common::{Clock, Kind, Regional, Reset},
    mapper::{vrc_irq::VrcIrq, Mapped, MappedRead, MappedWrite, Mapper, MemMap},
    mem::MemBanks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Vrc6Revision {
    /// VRC6a
    A,
    /// VRC6b
    B,
}

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc6Regs {
    banking_mode: u8,
    prg: [usize; 4],
    chr: [usize; 8],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc6 {
    regs: Vrc6Regs,
    revision: Vrc6Revision,
    mirroring: Mirroring,
    irq: VrcIrq,
    audio: Vrc6Audio,
    nt_banks: [usize; 4],
    chr_banks: MemBanks,
    prg_ram_banks: MemBanks,
    prg_rom_banks: MemBanks,
}

impl Vrc6 {
    const PRG_RAM_SIZE: usize = 8 * 1024;
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;

    pub fn load(cart: &mut Cart, revision: Vrc6Revision) -> Mapper {
        if !cart.has_prg_ram() {
            cart.add_prg_ram(Self::PRG_RAM_SIZE);
        }
        let mut vrc6 = Self {
            regs: Vrc6Regs::default(),
            revision,
            mirroring: cart.mirroring(),
            irq: VrcIrq::default(),
            audio: Vrc6Audio::new(),
            nt_banks: [0; 4],
            prg_ram_banks: MemBanks::new(0x6000, 0x7FFF, cart.prg_ram.len(), Self::PRG_RAM_SIZE),
            prg_rom_banks: MemBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_WINDOW),
            chr_banks: MemBanks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_WINDOW),
        };
        let last_bank = vrc6.prg_rom_banks.last();
        vrc6.prg_rom_banks.set(3, last_bank);
        vrc6.into()
    }

    #[inline]
    #[must_use]
    const fn prg_ram_enabled(&self) -> bool {
        self.regs.banking_mode & 0x80 == 0x80
    }

    #[inline]
    fn set_nametables(&mut self, nametables: &[usize]) {
        for (bank, page) in nametables.iter().enumerate() {
            self.set_nametable_page(bank, *page);
        }
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
        match self.mirroring {
            Mirroring::Vertical => self.set_nametables(&[0, 1, 0, 1]),
            Mirroring::Horizontal => self.set_nametables(&[0, 0, 1, 1]),
            Mirroring::SingleScreenA => self.set_nametables(&[0, 0, 0, 0]),
            Mirroring::SingleScreenB => self.set_nametables(&[1, 1, 1, 1]),
            Mirroring::FourScreen => self.set_nametables(&[0, 1, 2, 3]),
        }
    }

    #[inline]
    fn set_nametable_page(&mut self, bank: usize, page: usize) {
        self.nt_banks[bank] = page;
    }

    fn update_chr_banks(&mut self) {
        let (mask, or_mask) = if self.regs.banking_mode & 0x20 == 0x20 {
            (0xFE, 1)
        } else {
            (0xFF, 0)
        };

        match self.regs.banking_mode & 0x03 {
            0 => {
                self.chr_banks.set(0, self.regs.chr[0]);
                self.chr_banks.set(1, self.regs.chr[1]);
                self.chr_banks.set(2, self.regs.chr[2]);
                self.chr_banks.set(3, self.regs.chr[3]);
                self.chr_banks.set(4, self.regs.chr[4]);
                self.chr_banks.set(5, self.regs.chr[5]);
                self.chr_banks.set(6, self.regs.chr[6]);
                self.chr_banks.set(7, self.regs.chr[7]);
            }
            1 => {
                self.chr_banks.set(0, self.regs.chr[0] & mask);
                self.chr_banks.set(1, (self.regs.chr[0] & mask) | or_mask);
                self.chr_banks.set(2, self.regs.chr[1] & mask);
                self.chr_banks.set(3, (self.regs.chr[1] & mask) | or_mask);
                self.chr_banks.set(4, self.regs.chr[2] & mask);
                self.chr_banks.set(5, (self.regs.chr[2] & mask) | or_mask);
                self.chr_banks.set(6, self.regs.chr[3] & mask);
                self.chr_banks.set(7, (self.regs.chr[3] & mask) | or_mask);
            }
            _ => {
                self.chr_banks.set(0, self.regs.chr[0]);
                self.chr_banks.set(1, self.regs.chr[1]);
                self.chr_banks.set(2, self.regs.chr[2]);
                self.chr_banks.set(3, self.regs.chr[3]);
                self.chr_banks.set(4, self.regs.chr[4] & mask);
                self.chr_banks.set(5, (self.regs.chr[4] & mask) | or_mask);
                self.chr_banks.set(6, self.regs.chr[5] & mask);
                self.chr_banks.set(7, (self.regs.chr[5] & mask) | or_mask);
            }
        }

        if self.regs.banking_mode & 0x10 == 0x10 {
            // CHR-ROM
            self.set_mirroring(Mirroring::FourScreen);
            match self.regs.banking_mode & 0x2F {
                0x20 | 0x27 => {
                    self.set_nametable_page(0, self.regs.chr[6] & 0xFE);
                    self.set_nametable_page(1, (self.regs.chr[6] & 0xFE) | 1);
                    self.set_nametable_page(2, self.regs.chr[7] & 0xFE);
                    self.set_nametable_page(3, (self.regs.chr[7] & 0xFE) | 1);
                }
                0x23 | 0x24 => {
                    self.set_nametable_page(0, self.regs.chr[6] & 0xFE);
                    self.set_nametable_page(1, self.regs.chr[7] & 0xFE);
                    self.set_nametable_page(2, (self.regs.chr[6] & 0xFE) | 1);
                    self.set_nametable_page(3, (self.regs.chr[7] & 0xFE) | 1);
                }
                0x28 | 0x2F => {
                    self.set_nametable_page(0, self.regs.chr[6] & 0xFE);
                    self.set_nametable_page(1, self.regs.chr[6] & 0xFE);
                    self.set_nametable_page(2, self.regs.chr[7] & 0xFE);
                    self.set_nametable_page(3, self.regs.chr[7] & 0xFE);
                }
                0x2B | 0x2C => {
                    self.set_nametable_page(0, (self.regs.chr[6] & 0xFE) | 1);
                    self.set_nametable_page(1, (self.regs.chr[7] & 0xFE) | 1);
                    self.set_nametable_page(2, (self.regs.chr[6] & 0xFE) | 1);
                    self.set_nametable_page(3, (self.regs.chr[7] & 0xFE) | 1);
                }
                _ => match self.regs.banking_mode & 0x07 {
                    0 | 6 | 7 => {
                        self.set_nametable_page(0, self.regs.chr[6]);
                        self.set_nametable_page(1, self.regs.chr[6]);
                        self.set_nametable_page(2, self.regs.chr[7]);
                        self.set_nametable_page(3, self.regs.chr[7]);
                    }
                    1 | 5 => {
                        self.set_nametable_page(0, self.regs.chr[4]);
                        self.set_nametable_page(1, self.regs.chr[5]);
                        self.set_nametable_page(2, self.regs.chr[6]);
                        self.set_nametable_page(3, self.regs.chr[7]);
                    }
                    2 | 3 | 4 => {
                        self.set_nametable_page(0, self.regs.chr[6]);
                        self.set_nametable_page(1, self.regs.chr[7]);
                        self.set_nametable_page(2, self.regs.chr[6]);
                        self.set_nametable_page(3, self.regs.chr[7]);
                    }
                    _ => unreachable!("impossible banking mode: {}", self.regs.banking_mode),
                },
            }
        } else {
            // CIRAM
            match self.regs.banking_mode & 0x2F {
                0x20 | 0x27 => self.set_mirroring(Mirroring::Vertical),
                0x23 | 0x24 => self.set_mirroring(Mirroring::Horizontal),
                0x28 | 0x2F => self.set_mirroring(Mirroring::SingleScreenA),
                0x2B | 0x2C => self.set_mirroring(Mirroring::SingleScreenB),
                _ => {
                    self.set_mirroring(Mirroring::FourScreen);
                    match self.regs.banking_mode & 0x07 {
                        0 | 6 | 7 => {
                            self.set_nametable_page(0, self.regs.chr[6] & 0x01);
                            self.set_nametable_page(1, self.regs.chr[6] & 0x01);
                            self.set_nametable_page(2, self.regs.chr[7] & 0x01);
                            self.set_nametable_page(3, self.regs.chr[7] & 0x01);
                        }
                        1 | 5 => {
                            self.set_nametable_page(0, self.regs.chr[4] & 0x01);
                            self.set_nametable_page(1, self.regs.chr[5] & 0x01);
                            self.set_nametable_page(2, self.regs.chr[6] & 0x01);
                            self.set_nametable_page(3, self.regs.chr[7] & 0x01);
                        }
                        2 | 3 | 4 => {
                            self.set_nametable_page(0, self.regs.chr[6] & 0x01);
                            self.set_nametable_page(1, self.regs.chr[7] & 0x01);
                            self.set_nametable_page(2, self.regs.chr[6] & 0x01);
                            self.set_nametable_page(3, self.regs.chr[7] & 0x01);
                        }
                        _ => unreachable!("impossible banking mode: {}", self.regs.banking_mode),
                    }
                }
            }
        }
    }
}

impl Mapped for Vrc6 {
    #[inline]
    fn irq_pending(&self) -> bool {
        self.irq.pending()
    }

    #[inline]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    #[inline]
    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MemMap for Vrc6 {
    // PPU $0000..=$03FF 1K switchable CHR-ROM bank
    // PPU $0400..=$07FF 1K switchable CHR-ROM bank
    // PPU $0800..=$0BFF 1K switchable CHR-ROM bank
    // PPU $0C00..=$0FFF 1K switchable CHR-ROM bank
    // PPU $1000..=$13FF 1K switchable CHR-ROM bank
    // PPU $1400..=$17FF 1K switchable CHR-ROM bank
    // PPU $1800..=$1BFF 1K switchable CHR-ROM bank
    // PPU $1C00..=$1FFF 1K switchable CHR-ROM bank
    // PPU $2000..=$3EFF Switchable Nametables
    //
    // CPU $6000..=$7FFF 8K PRG-RAM bank, fixed
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$DFFF 8K switchable PRG-ROM bank
    // CPU $E000..=$FFFF 8K PRG-ROM bank, fixed to the last bank

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x2000..=0x3EFF => {
                let addr = addr - 0x2000;
                let a10 = (self.nt_banks[((addr >> 10) & 0x03) as usize] << 10) as u16;
                let addr = a10 | (!a10 & addr);
                if self.regs.banking_mode & 0x10 == 0x00 {
                    MappedRead::CIRam(addr.into())
                } else {
                    MappedRead::Chr(self.chr_banks.translate(addr))
                }
            }
            0x6000..=0x7FFF if self.prg_ram_enabled() => {
                MappedRead::PrgRam(self.prg_ram_banks.translate(addr))
            }
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::None,
        }
    }

    fn map_write(&mut self, mut addr: u16, val: u8) -> MappedWrite {
        if self.prg_ram_enabled() && matches!(addr, 0x6000..=0x7FFF) {
            return MappedWrite::PrgRam(self.prg_ram_banks.translate(addr), val);
        }

        if self.revision == Vrc6Revision::B {
            // Revision B swaps A0 and A1 lines
            addr = (addr & 0xFFFC) | ((addr & 0x01) << 1) | ((addr & 0x02) >> 1);
        }

        // Only A0, A1 and A12-15 are used for registers, remaining addresses are mirrored.
        match addr & 0xF003 {
            0x8000..=0x8003 => {
                // [.... PPPP]
                //       ||||
                //       ++++- Select 16 KB PRG-ROM bank at $8000-$BFFF
                self.prg_rom_banks
                    .set_range(0, 1, ((val & 0x0F) << 1).into());
            }
            0x9000..=0x9003 | 0xA000..=0xA002 | 0xB000..=0xB002 => {
                self.audio.write_register(addr, val);
            }
            0xB003 => {
                // [W.PN MMDD]
                //  | || ||||
                //  | || ||++- PPU banking mode; see below
                //  | || ++--- Mirroring varies by banking mode, see below
                //  | |+------ 1: Nametables come from CHRROM, 0: Nametables come from CIRAM
                //  | +------- CHR A10 is 1: subject to further rules 0: according to the latched value
                //  +--------- PRG RAM enable
                self.regs.banking_mode = val;
                self.update_chr_banks();
            }
            0xC000..=0xC003 => {
                // [...P PPPP]
                //     | ||||
                //     +-++++- Select 8 KB PRG-ROM bank at $C000-$DFFF
                self.prg_rom_banks.set(2, (val & 0x1F).into());
            }
            0xD000..=0xD003 => {
                self.regs.chr[(addr & 0x03) as usize] = val.into();
                self.update_chr_banks();
            }
            0xE000..=0xE003 => {
                self.regs.chr[(4 + (addr & 0x03)) as usize] = val.into();
                self.update_chr_banks();
            }
            0xF000 => self.irq.write_reload(val),
            0xF001 => self.irq.write_control(val),
            0xF002 => self.irq.acknowledge(),
            _ => (),
        }
        MappedWrite::None
    }
}

impl Audio for Vrc6 {
    #[inline]
    #[must_use]
    fn output(&self) -> f32 {
        self.audio.output()
    }
}

impl Clock for Vrc6 {
    fn clock(&mut self) -> usize {
        self.irq.clock();
        self.audio.clock();
        1
    }
}

impl Reset for Vrc6 {
    fn reset(&mut self, kind: Kind) {
        self.irq.reset(kind);
        self.audio.reset(kind);
    }
}

impl Regional for Vrc6 {}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc6Audio {
    pulse1: Vrc6Pulse,
    pulse2: Vrc6Pulse,
    saw: Vrc6Saw,
    halt: bool,
    out: f32,
    last_out: f32,
}

impl Default for Vrc6Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Vrc6Audio {
    const fn new() -> Self {
        Self {
            pulse1: Vrc6Pulse::new(),
            pulse2: Vrc6Pulse::new(),
            saw: Vrc6Saw::new(),
            halt: false,
            out: 0.0,
            last_out: 0.0,
        }
    }

    #[inline]
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

impl Clock for Vrc6Audio {
    fn clock(&mut self) -> usize {
        if !self.halt {
            self.pulse1.clock();
            self.pulse2.clock();
            self.saw.clock();

            self.out = self.pulse1.volume() + self.pulse2.volume() + self.saw.volume();
        }
        1
    }
}

impl Reset for Vrc6Audio {
    fn reset(&mut self, _kind: Kind) {
        self.last_out = 0.0;
        self.halt = false;
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc6Pulse {
    enabled: bool,
    volume: u8,
    duty_cycle: u8,
    ignore_duty: bool,
    frequency: u16,
    timer: u16,
    step: u8,
    freq_shift: u8,
}

impl Default for Vrc6Pulse {
    fn default() -> Self {
        Self::new()
    }
}

impl Vrc6Pulse {
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

    #[inline]
    fn set_freq_shift(&mut self, val: u8) {
        self.freq_shift = val;
    }

    #[inline]
    fn volume(&self) -> f32 {
        if self.enabled && (self.ignore_duty || self.step <= self.duty_cycle) {
            f32::from(self.volume)
        } else {
            0.0
        }
    }
}

impl Clock for Vrc6Pulse {
    fn clock(&mut self) -> usize {
        if self.enabled {
            self.timer -= 1;
            if self.timer == 0 {
                self.step = (self.step + 1) & 0x0F;
                self.timer = (self.frequency >> self.freq_shift) + 1;
                return 1;
            }
        }
        0
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc6Saw {
    enabled: bool,
    accum: u8,
    accum_rate: u8,
    frequency: u16,
    timer: u16,
    step: u8,
    freq_shift: u8,
}

impl Default for Vrc6Saw {
    fn default() -> Self {
        Self::new()
    }
}

impl Vrc6Saw {
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

    #[inline]
    fn set_freq_shift(&mut self, val: u8) {
        self.freq_shift = val;
    }

    #[inline]
    fn volume(&self) -> f32 {
        if self.enabled {
            f32::from(self.accum >> 3)
        } else {
            0.0
        }
    }
}

impl Clock for Vrc6Saw {
    fn clock(&mut self) -> usize {
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
                return 1;
            }
        }
        0
    }
}
