//! `Namco163` (Mapper 019).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_019>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sample, Sram},
    mapper::{self, Map, Mapper, MapperOps},
    memory::ConstArray,
    memory::{Memory, Src},
    ppu::Mirroring,
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
    pub regs: Regs,
    pub board: Board,
    pub mapper_num: u16,
    pub submapper_num: u8,
    pub audio: Audio,
    pub auto_detect_board: bool,
    pub mirroring: Mirroring,
    pub prg_ram_written_to: bool,
    /// Whether each of the twelve 1K slots covering $0000-$2FFF reads CIRAM instead of CHR-ROM.
    pub nt_bank_enable: [bool; 12],
    /// Bank selected by each of those twelve slots.
    pub chr_banks: [u8; 12],
    /// 3x 8K PRG-ROM banks. $E000 is fixed to the last bank.
    pub prg_banks: [u8; 3],
}

impl Namco163 {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;

    /// Load `Namco163` from `Cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut auto_detect_board = false;
        let board = match cart.mapper_num() {
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
        };
        let mut board = Self {
            regs: Regs::default(),
            board,
            mapper_num: cart.mapper_num(),
            submapper_num: cart.submapper_num(),
            audio: Audio::new(),
            auto_detect_board,
            mirroring: cart.mirroring(),
            prg_ram_written_to: false,
            nt_bank_enable: [false; 12],
            chr_banks: [0; 12],
            prg_banks: [0; 3],
        };
        board.sync(&mut cart.memory);
        Ok(board.into())
    }

    /// Whether PRG-RAM at $6000 currently accepts writes.
    const fn prg_ram_writable(&self) -> bool {
        match self.board {
            Board::Namco163 => true,
            Board::Namco175 => self.regs.prg_ram_protect & 0x01 == 0x01,
            _ => false,
        }
    }

    /// Whether PRG-RAM at $6000 is mapped at all.
    const fn prg_ram_readable(&self) -> bool {
        matches!(self.board, Board::Namco163 | Board::Namco175)
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
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ | MapperOps::AUDIO | MapperOps::SERVES_PRG_READS
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.regs.irq_pending
    }

    /// Internal sound RAM is battery-backed on this board and shares the PRG-RAM save file.
    fn save_sram(&self, memory: &Memory, path: &std::path::Path) -> crate::fs::Result<()> {
        crate::fs::save(
            path,
            &(
                memory.region_ref(Src::PrgRam).to_vec(),
                self.audio.ram.to_vec(),
            ),
        )
    }

    fn load_sram(&mut self, memory: &mut Memory, path: &std::path::Path) -> crate::fs::Result<()> {
        let (prg_ram, audio_ram) = crate::fs::load::<(Vec<u8>, Vec<u8>)>(path)?;
        let ram = memory.region_mut(Src::PrgRam);
        let len = ram.len().min(prg_ram.len());
        ram[..len].copy_from_slice(&prg_ram[..len]);
        for (dst, src) in self.audio.ram.iter_mut().zip(&audio_ram) {
            *dst = *src;
        }
        Ok(())
    }

    /// Audio registers and the IRQ counter live in the expansion range and are not memory.
    fn prg_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x4800..=0x4FFF => Some(self.audio.read_register(addr)),
            0x5000..=0x57FF => Some((self.regs.irq_counter & 0xFF) as u8),
            0x5800..=0x5FFF => Some((self.regs.irq_counter >> 8) as u8),
            _ => None,
        }
    }

    fn prg_peek(&self, addr: u16) -> Option<u8> {
        match addr {
            0x4800..=0x4FFF => Some(self.audio.peek_register(addr)),
            0x5000..=0x57FF => Some((self.regs.irq_counter & 0xFF) as u8),
            0x5800..=0x5FFF => Some((self.regs.irq_counter >> 8) as u8),
            _ => None,
        }
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
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
                // The data store already happened in `Bus`; this only tracks board detection.
                self.prg_ram_written_to = true;
                if self.board == Board::Namco340 {
                    self.maybe_set_board(Board::Unknown);
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
                } else {
                    let bank = ((addr - 0x8000) >> 11) as usize;
                    // The eight CHR registers at $8000-$BFFF can only redirect a bank to CIRAM on
                    // a Namco163, and only when the matching nametable mode bit allows it. The
                    // four nametable registers at $C000-$DFFF apply the >= $E0 rule on every
                    // variant - gating those on Namco163 too left the 340 reading CHR-ROM for its
                    // nametables.
                    let nt_bank_enable = val >= 0xE0
                        && match addr {
                            0x8000..=0x9FFF => {
                                !self.regs.nt_select_lo && self.board == Board::Namco163
                            }
                            0xA000..=0xBFFF => {
                                !self.regs.nt_select_hi && self.board == Board::Namco163
                            }
                            _ => true,
                        };
                    self.nt_bank_enable[bank] = nt_bank_enable;
                    if nt_bank_enable {
                        self.chr_banks[bank] = val & 0x01;
                    } else {
                        self.chr_banks[bank] = val;
                    }
                }
            }
            0xE000..=0xE7FF => {
                if val & 0x80 == 0x80 || (val & 0x40 == 0x40 && self.board != Board::Namco163) {
                    self.maybe_set_board(Board::Namco340);
                }

                self.prg_banks[0] = val & 0x3F;

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
                self.prg_banks[1] = val & 0x3F;

                if self.board == Board::Namco163 {
                    self.regs.nt_select_lo = (val & 0x40) == 0x40;
                    self.regs.nt_select_hi = (val & 0x80) == 0x80;
                }
            }
            0xF000..=0xF7FF => self.prg_banks[2] = val & 0x3F,
            0xF800..=0xFFFF => {
                self.maybe_set_board(Board::Namco163);
                if self.board == Board::Namco163 {
                    self.regs.prg_ram_protect = val;

                    self.audio.write_register(addr, val);
                }
            }
            _ => (),
        }
        self.sync(memory);
    }

    fn sync(&mut self, memory: &mut Memory) {
        // Twelve 1K slots cover $0000-$2FFF; each independently selects CHR-ROM or CIRAM, which is
        // how this board expresses both pattern banking and nametable layout.
        for slot in 0..12 {
            let src = if self.nt_bank_enable[slot] {
                Src::CiRam
            } else {
                Src::Chr
            };
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(self.chr_banks[slot]), src);
            // $3000-$3FFF mirrors $2000-$2FFF.
            if slot >= 8 {
                memory.map_chr(
                    addr + 0x1000,
                    Self::CHR_WINDOW,
                    i32::from(self.chr_banks[slot]),
                    src,
                );
            }
        }

        for (slot, bank) in self.prg_banks.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, i32::from(*bank), Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);

        if self.prg_ram_readable() {
            memory.map_prg(0x6000, Self::PRG_WINDOW, 0, Src::PrgRam);
            memory.set_prg_writable(0x6000, Self::PRG_WINDOW, self.prg_ram_writable());
        } else {
            memory.unmap_prg(0x6000, Self::PRG_WINDOW);
        }
    }
}

impl Reset for Namco163 {
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            self.regs = Regs::default();
        }
        for bank in 8..12 {
            self.nt_bank_enable[bank] = true;
            // Preserves the previous expression `((bank - 8) * 0x0400) & 0x03FF`, which is zero
            // for every bank since 0x400 & 0x3FF == 0 - it looks like page indices and byte
            // offsets were conflated, but games program these registers immediately anyway.
            self.chr_banks[bank] = 0;
        }
        self.prg_ram_written_to = false;
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

impl Sram for Namco163 {}

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
