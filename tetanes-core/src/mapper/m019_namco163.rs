//! `Namco163` (Mapper 019).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_019>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sample, Sram},
    fs,
    mapper::{self, Map, Mapper},
    mem::{BankAccess, Banks, ConstArray, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// `Namco163` board.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Board {
    #[default]
    Unknown,
    Namco163,
    Namco175,
    Namco340,
}

/// `Namco163` registers.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    pub irq_counter: u16,
    pub irq_pending: bool,
    pub nt_select_lo: bool,
    pub nt_select_hi: bool,
    pub prg_ram_protect: u8,
}

/// `Namco163` (Mapper 019).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Namco163 {
    pub chr_rom: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub prg_ram: Memory<Box<[u8]>>,
    pub regs: Regs,
    pub board: Board,
    pub mapper_num: u16,
    pub submapper_num: u8,
    pub audio: Audio,
    pub auto_detect_board: bool,
    pub mirroring: Mirroring,
    pub prg_ram_written_to: bool,
    pub nt_bank_enable: [bool; 12],
    pub chr_banks: Banks,
    pub prg_ram_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl Namco163 {
    const PRG_WINDOW: usize = 8 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;

    /// Load `Namco163` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let mut auto_detect_board = false;
        let prg_ram = cart.prg_ram_or_default(Self::PRG_RAM_SIZE);
        let chr_banks = Banks::new(0x0000, 0x3FFF, chr_rom.len(), Self::CHR_WINDOW)?;
        let prg_ram_banks = Banks::new(0x6000, 0x7FFF, prg_ram.len(), Self::PRG_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_WINDOW)?;
        let mut namco163 = Self {
            chr_rom,
            prg_rom,
            prg_ram,
            regs: Regs::default(),
            board: match cart.mapper_num() {
                19 => {
                    auto_detect_board = cart.game_info.is_none();
                    Board::Namco163
                }
                210 => match cart.submapper_num() {
                    1 => Board::Namco175,
                    2 => Board::Namco340,
                    _ => {
                        auto_detect_board = true;
                        Board::Unknown
                    }
                },
                _ => Board::Unknown,
            },
            mapper_num: cart.mapper_num(),
            submapper_num: cart.submapper_num(),
            audio: Audio::new(),
            auto_detect_board,
            mirroring: cart.mirroring(),
            prg_ram_written_to: false,
            nt_bank_enable: [false; 12],
            chr_banks,
            prg_ram_banks,
            prg_rom_banks,
        };
        // Default 0x2000.=0x2FFF to NTRAM
        for bank in 8..12 {
            namco163.nt_bank_enable[bank] = true;
            namco163.chr_banks.set(bank, ((bank - 8) * 0x0400) & 0x03FF);
        }
        namco163.prg_rom_banks.set(3, namco163.prg_rom_banks.last());
        namco163.update_prg_ram_access();
        Ok(namco163.into())
    }

    fn update_prg_ram_access(&mut self) {
        if self.prg_ram_banks.banks_len() == 0 {
            return;
        }
        let access = |read_write| {
            if read_write {
                BankAccess::ReadWrite
            } else {
                BankAccess::Read
            }
        };
        let write_protect = self.regs.prg_ram_protect;
        match self.board {
            Board::Namco163 => {
                self.prg_ram_banks.set_access_range(0, 3, access(true));
            }
            Board::Namco175 => {
                self.prg_ram_banks
                    .set_access_range(0, 3, access(write_protect & 0x01 == 0x01));
            }
            _ => {
                self.prg_ram_banks.set_access_range(0, 3, BankAccess::None);
            }
        }
    }

    #[inline]
    fn maybe_set_board(&mut self, board: Board) {
        if self.auto_detect_board
            && (!self.prg_ram_written_to || self.board != Board::Namco340)
            && self.board != board
        {
            tracing::debug!("auto detecting board: {board:?}");
            self.board = board;
        }
    }
}

impl Map for Namco163 {
    // PPU $0000..=$03FF 1K CHR Bank 1 Switchable
    // PPU $0400..=$07FF 1K CHR Bank 2 Switchable
    // PPU $0800..=$0BFF 1K CHR Bank 3 Switchable
    // PPU $0C00..=$0FFF 1K CHR Bank 4 Switchable
    // PPU $1000..=$13FF 1K CHR Bank 5 Switchable
    // PPU $1400..=$17FF 1K CHR Bank 6 Switchable
    // PPU $1800..=$1BFF 1K CHR Bank 7 Switchable
    // PPU $1C00..=$1FFF 1K CHR Bank 8 Switchable
    // PPU $2000..=$23FF 1K CHR Bank 9 Switchable
    // PPU $2400..=$27FF 1K CHR Bank 10 Switchable
    // PPU $2800..=$2BFF 1K CHR Bank 11 Switchable
    // PPU $2C00..=$2FFF 1K CHR Bank 12 Switchable
    //
    // CPU $6000..=$7FFF 8K PRG-RAM Bank, if WRAM is present
    // CPU $8000..=$9FFF 8K PRG-ROM Bank 1 Switchable
    // CPU $A000..=$BFFF 8K PRG-ROM Bank 2 Switchable
    // CPU $C000..=$DFFF 8K PRG-ROM Bank 3 Switchable
    // CPU $E000..=$FFFF 8K PRG-ROM Bank 4, fixed to last

    // $0400..=$07FF bank 1 > page N -> addr + page * $0400
    // $0800..=$0BFF bank 2 -> page N -> addr + page * $0400
    // $0C00..=$0FFF bank 3 -> page N -> addr + page * $0400
    // $1000..=$13FF bank 4 -> page N -> addr + page * $0400
    // $1400..=$17FF bank 5 -> page N -> addr + page * $0400
    // $1800..=$1BFF bank 6 -> page N -> addr + page * $0400
    // $1C00..=$1FFF bank 7 -> page N -> addr + page * $0400
    // $2000..=$23FF bank 8 -> page N -> addr + page * $0400
    // $2400..=$27FF bank 9 -> page N -> addr + page * $0400
    // $2800..=$2BFF bank 10 -> page N -> addr + page * $0400
    // $2C00..=$2FFF bank 11 -> page N -> addr + page * $0400

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x3EFF => {
                let bank = addr >> 10;
                let addr = self.chr_banks.translate(addr);
                if self.nt_bank_enable[bank as usize] {
                    ciram[addr]
                } else {
                    self.chr_rom[addr]
                }
            }
            _ => 0,
        }
    }

    /// Read a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x4800..=0x4FFF => self.audio.read_register(addr),
            _ => self.prg_peek(addr),
        }
    }

    /// Peek a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_peek(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_banks.readable(addr) {
                    self.prg_ram[self.prg_ram_banks.translate(addr)]
                } else {
                    0
                }
            }
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => match addr & 0xF800 {
                0x4800 => self.audio.peek_register(addr),
                0x5000 => (self.regs.irq_counter & 0xFF) as u8,
                0x5800 => (self.regs.irq_counter >> 8) as u8,
                _ => 0,
            },
        }
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        if let 0x0000..=0x3EFF = addr {
            let bank = addr >> 10;
            let addr = self.chr_banks.translate(addr);
            if self.nt_bank_enable[bank as usize] {
                ciram[addr] = val;
            }
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    fn prg_write(&mut self, addr: u16, val: u8) {
        match addr {
            0x4800..=0x4FFF => {
                self.maybe_set_board(Board::Namco163);
                self.audio.write_register(addr, val)
            }
            0x5000..=0x57FF => {
                self.maybe_set_board(Board::Namco163);
                self.regs.irq_counter = (self.regs.irq_counter & 0xFF00) | u16::from(val);
                self.regs.irq_pending = false;
            }
            0x5800..=0x5FFF => {
                self.maybe_set_board(Board::Namco163);
                self.regs.irq_counter = (self.regs.irq_counter & 0xFF) | (u16::from(val) << 8);
                self.regs.irq_pending = false;
            }
            0x6000..=0x7FFF => {
                self.prg_ram_written_to = true;
                if self.board == Board::Namco340 {
                    self.maybe_set_board(Board::Unknown);
                }
                if self.prg_ram_banks.writable(addr) {
                    self.prg_ram[self.prg_ram_banks.translate(addr)] = val;
                }
            }
            0x8000..=0xDFFF => {
                if addr >= 0xC800 {
                    self.maybe_set_board(Board::Namco163);
                } else if addr >= 0xC000 && self.board != Board::Namco163 {
                    self.maybe_set_board(Board::Namco175);
                }

                if addr >= 0xC000 && self.board == Board::Namco175 {
                    self.regs.prg_ram_protect = val;
                    self.update_prg_ram_access();
                } else {
                    let bank = ((addr - 0x8000) >> 11) as usize;
                    let nt_select = match addr {
                        0x8000..=0x9FFF => !self.regs.nt_select_lo,
                        0xA000..=0xBFFF => !self.regs.nt_select_hi,
                        _ => true,
                    };
                    let nt_bank_enable = nt_select && val >= 0xE0 && self.board == Board::Namco163;
                    self.nt_bank_enable[bank] = nt_bank_enable;
                    if nt_bank_enable {
                        self.chr_banks.set(bank, (val & 0x01).into());
                    } else {
                        self.chr_banks.set(bank, val.into());
                    }
                }
            }
            0xE000..=0xE7FF => {
                if val & 0x80 == 0x80 || (val & 0x40 == 0x40 && self.board != Board::Namco163) {
                    self.maybe_set_board(Board::Namco340);
                }

                self.prg_rom_banks.set(0, (val & 0x3F).into());

                match self.board {
                    Board::Namco340 => {
                        self.mirroring = match (val & 0xC0) >> 6 {
                            0b00 => Mirroring::SingleScreenA,
                            0b01 => Mirroring::Vertical,
                            0b10 => Mirroring::Horizontal,
                            _ => Mirroring::SingleScreenB,
                        };
                    }
                    Board::Namco163 => self.audio.write_register(addr, val),
                    _ => (),
                }
            }
            0xE800..=0xEFFF => {
                self.prg_rom_banks.set(1, (val & 0x3F).into());

                if self.board == Board::Namco163 {
                    self.regs.nt_select_lo = (val & 0x40) == 0x40;
                    self.regs.nt_select_hi = (val & 0x80) == 0x80;
                }
            }
            0xF000..=0xF7FF => self.prg_rom_banks.set(2, (val & 0x3F).into()),
            0xF800..=0xFFFF => {
                self.maybe_set_board(Board::Namco163);
                if self.board == Board::Namco163 {
                    self.regs.prg_ram_protect = val;
                    self.update_prg_ram_access();
                    self.audio.write_register(addr, val);
                }
            }
            _ => (),
        }
    }

    /// Whether an IRQ is pending acknowledgement.
    fn irq_pending(&self) -> bool {
        self.regs.irq_pending
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Namco163 {
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            self.regs = Regs::default();
        }
        for bank in 8..12 {
            self.nt_bank_enable[bank] = true;
            self.chr_banks.set(bank, ((bank - 8) * 0x0400) & 0x03FF);
        }
        self.prg_ram_written_to = false;
        self.prg_rom_banks.set(3, self.prg_rom_banks.last());
        self.update_prg_ram_access();
        self.audio = Audio::new();
    }
}

impl Clock for Namco163 {
    fn clock(&mut self) {
        if self.regs.irq_counter & 0x8000 > 0 && self.regs.irq_counter & 0x7FFF != 0x7FFF {
            self.regs.irq_counter = self.regs.irq_counter.wrapping_add(1);
            if self.regs.irq_counter & 0x7FFF == 0x7FFF {
                self.regs.irq_pending = true;
            }
        }
        if self.board == Board::Namco163 {
            self.audio.clock();
        }
    }
}

impl Regional for Namco163 {}

impl Sram for Namco163 {
    fn save(&self, path: impl AsRef<std::path::Path>) -> fs::Result<()> {
        fs::save(path.as_ref(), &(&self.prg_ram, &self.audio.ram))
    }

    fn load(&mut self, path: impl AsRef<std::path::Path>) -> fs::Result<()> {
        fs::load::<(Memory<Box<[u8]>>, ConstArray<u8, 0x80>)>(path.as_ref()).map(
            |(prg_ram, audio_ram)| {
                self.prg_ram = prg_ram;
                self.audio.ram = audio_ram;
            },
        )
    }
}

impl Sample for Namco163 {
    fn output(&self) -> f32 {
        self.audio.output()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Audio {
    pub ram: ConstArray<u8, 0x80>,
    pub addr: usize,
    pub auto_increment: bool,
    pub disabled: bool,
    pub update_counter: u8,
    pub current_channel: i8,
    pub channel_out: [f32; Self::CHANNEL_COUNT],
    pub out: f32,
    #[serde(skip, default)]
    phase_ext: [u32; Self::CHANNEL_COUNT],
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    const CHANNEL_COUNT: usize = 8;

    const REG_FREQ_LOW: usize = 0x00;
    const REG_FREQ_MID: usize = 0x02;
    const REG_FREQ_HIGH: usize = 0x04;
    const REG_WAVE_LEN: usize = 0x04;
    const REG_WAVE_ADDR: usize = 0x06;
    const REG_VOLUME: usize = 0x07;

    pub fn new() -> Self {
        Self {
            ram: ConstArray::new(),
            addr: 0,
            auto_increment: false,
            disabled: false,
            update_counter: 0,
            current_channel: 7,
            channel_out: [0.0; Self::CHANNEL_COUNT],
            out: 0.0,
            phase_ext: [0; Self::CHANNEL_COUNT],
        }
    }

    #[must_use]
    pub fn read_register(&mut self, addr: u16) -> u8 {
        let val = self.peek_register(addr);
        if self.auto_increment {
            self.addr = (self.addr + 1) & 0x7F;
        }
        val
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn peek_register(&self, addr: u16) -> u8 {
        if matches!(addr, 0x4800..=0x4FFF) {
            self.ram[self.addr]
        } else {
            0
        }
    }

    pub fn write_register(&mut self, addr: u16, val: u8) {
        match addr {
            0x4800..=0x4FFF => {
                self.ram[self.addr] = val;
                if self.auto_increment {
                    self.addr = (self.addr + 1) & 0x7F;
                }
            }
            0xE000..=0xE7FF => self.disabled = val & 0x40 == 0x40,
            0xF800..=0xFFFF => {
                self.addr = (val & 0x7F).into();
                self.auto_increment = val & 0x80 == 0x80;
            }
            _ => (),
        }
    }

    #[must_use]
    #[inline]
    pub const fn output(&self) -> f32 {
        // TODO: -40db - it's not accurate according to https://www.nesdev.org/wiki/Namco_163_audio#Mixing
        // but it's way too loud otherwise. Should fix root cause and update to use NES 2.0
        // submapper_num, if set
        0.0001 * self.out
    }

    #[inline]
    fn update_output(&mut self) {
        // "Because the high frequency generated by the channel cycling can be unpleasant, and
        // emulation of high frequency audio can be difficult, it is often preferred to simply sum
        // the channel outputs, and divide the output volume by the number of active channels."
        // See: https://www.nesdev.org/wiki/Namco_163_audio#Mixing
        let channel_count = usize::from(self.channel_count());
        self.out = self.channel_out.iter().skip(7 - channel_count).sum::<f32>()
            / (channel_count + 1) as f32;
    }

    #[must_use]
    #[inline]
    const fn base_addr(&self) -> usize {
        (0x40 + self.current_channel * 0x08) as usize
    }

    #[must_use]
    #[inline]
    const fn phase(&self) -> u32 {
        self.phase_ext[self.current_channel as usize]
    }

    #[must_use]
    #[inline]
    fn wave_length(&self) -> u32 {
        let base_addr = self.base_addr();
        256 - u32::from(self.ram[base_addr + Self::REG_WAVE_LEN] & 0xFC)
    }

    #[must_use]
    #[inline]
    fn wave_address(&self) -> u32 {
        let base_addr = self.base_addr();
        u32::from(self.ram[base_addr + Self::REG_WAVE_ADDR])
    }

    #[must_use]
    #[inline]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    fn volume(&self) -> u8 {
        let base_addr = self.base_addr();
        self.ram[base_addr + Self::REG_VOLUME] & 0x0F
    }

    #[inline]
    const fn set_phase(&mut self, phase: u32) {
        self.phase_ext[self.current_channel as usize] = phase;
    }

    #[must_use]
    #[inline]
    fn frequency(&self) -> u32 {
        let base_addr = self.base_addr();
        let freq_high = u32::from(self.ram[base_addr + Self::REG_FREQ_HIGH] & 0x03) << 16;
        let freq_mid = u32::from(self.ram[base_addr + Self::REG_FREQ_MID]) << 8;
        let freq_low = u32::from(self.ram[base_addr + Self::REG_FREQ_LOW]);
        freq_high | freq_mid | freq_low
    }

    #[inline]
    fn update_channel(&mut self) {
        let mut phase = self.phase();
        let frequency = self.frequency();
        let wave_length = self.wave_length();
        let wave_addr = self.wave_address();
        let volume = self.volume();

        phase = (phase + frequency) % (wave_length << 16);
        let sample_addr = (((phase >> 16) + wave_addr) & 0xFF) as usize;
        let sample = if sample_addr & 0x01 == 0x01 {
            self.ram[sample_addr / 2] >> 4
        } else {
            self.ram[sample_addr / 2] & 0x0F
        };
        self.channel_out[self.current_channel as usize] =
            sample.wrapping_sub(8) as f32 * volume as f32;
        self.update_output();
        self.set_phase(phase);
    }

    #[must_use]
    #[inline]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    fn channel_count(&self) -> u8 {
        (self.ram[0x7F] >> 4) & 0x07
    }
}

impl Clock for Audio {
    fn clock(&mut self) {
        if !self.disabled {
            self.update_counter += 1;
            if self.update_counter == 15 {
                self.update_counter = 0;
                self.update_channel();

                self.current_channel -= 1;
                if self.current_channel < 7 - self.channel_count() as i8 {
                    self.current_channel = 7;
                }
            }
        }
    }
}
