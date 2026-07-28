//! `Namco163` (Mapper 019).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_019>

use crate::{
    cart::Cart,
    common::ResetKind,
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
        board.update_banks(&mut cart.memory);
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
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
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
    pub fn clock(&mut self) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (128 1K pages), 8K PRG-RAM, 64K CHR-ROM (64 1K pages), as a mapper 019 cart so
    /// the board auto-detects as a Namco163 rather than staying `Unknown`.
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 64 * 1024);
        cart.header.mapper_num = 19;
        let mapper = Namco163::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Arms the 15-bit counter `ticks` short of firing.
    fn arm_irq(mapper: &mut Mapper, cart: &mut Cart, ticks: u16) {
        let counter = 0x8000 | (0x7FFF - ticks);
        write(mapper, cart, 0x5000, (counter & 0xFF) as u8);
        write(mapper, cart, 0x5800, (counter >> 8) as u8);
    }

    /// Three switchable 8K windows and a fixed last bank at $E000. The bank registers live at
    /// $E000/$E800/$F000 - *not* at the window they control - which is the easy thing to get
    /// backwards.
    #[test]
    fn prg_windows_are_three_switchable_8k_banks_and_a_fixed_last() {
        let (mut mapper, mut cart) = load();
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120, "last bank at $E000");

        write(&mut mapper, &mut cart, 0xE000, 3);
        write(&mut mapper, &mut cart, 0xE800, 5);
        write(&mut mapper, &mut cart, 0xF000, 7);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 5 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120, "still fixed");
    }

    /// The IRQ counter is 15 bits with bit 15 as the enable, counting *up* to $7FFF - so the value
    /// written is not a period but a starting point.
    #[test]
    fn irq_counter_counts_up_to_7fff_when_bit_15_is_set() {
        let (mut mapper, mut cart) = load();
        arm_irq(&mut mapper, &mut cart, 15);

        for clock in 0..14 {
            mapper.clock();
            assert!(!mapper.irq_pending(), "too early, at clock {clock}");
        }
        mapper.clock();
        assert!(mapper.irq_pending(), "fires on the 15th clock");
    }

    /// With bit 15 clear the counter is frozen, and once it has fired it stops rather than wrapping.
    #[test]
    fn the_counter_is_frozen_while_disabled_and_after_firing() {
        let (mut mapper, mut cart) = load();

        // Bit 15 clear: armed 15 short of $7FFF, but disabled.
        write(&mut mapper, &mut cart, 0x5000, 0xF0);
        write(&mut mapper, &mut cart, 0x5800, 0x7F);
        for _ in 0..100 {
            mapper.clock();
        }
        assert!(!mapper.irq_pending(), "a disabled counter never fires");
        assert_eq!(
            prg_peek(&mapper, &cart, 0x5000),
            0xF0,
            "and never counts either"
        );

        arm_irq(&mut mapper, &mut cart, 1);
        mapper.clock();
        assert!(mapper.irq_pending());
        for _ in 0..100 {
            mapper.clock();
        }
        assert_eq!(
            prg_peek(&mapper, &cart, 0x5000),
            0xFF,
            "the counter halts at $7FFF instead of wrapping"
        );
    }

    /// The counter reads back through `prg_read`, which is the escape hatch for addresses no page
    /// entry can describe - these registers are in the $4800-$5FFF expansion range, not memory.
    #[test]
    fn the_counter_reads_back_through_the_prg_escape_hatch() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x5000, 0x34);
        write(&mut mapper, &mut cart, 0x5800, 0x12);
        assert_eq!(prg_peek(&mapper, &cart, 0x5000), 0x34, "low byte");
        assert_eq!(prg_peek(&mapper, &cart, 0x5800), 0x12, "high byte");
        // The whole $5000-$57FF and $5800-$5FFF ranges mirror the same register.
        assert_eq!(prg_peek(&mapper, &cart, 0x57FF), 0x34);
        assert_eq!(prg_peek(&mapper, &cart, 0x5FFF), 0x12);
    }

    /// Writing either half of the counter acknowledges a pending IRQ.
    #[test]
    fn writing_the_counter_acknowledges_the_irq() {
        let (mut mapper, mut cart) = load();
        arm_irq(&mut mapper, &mut cart, 1);
        mapper.clock();
        assert!(mapper.irq_pending());

        write(&mut mapper, &mut cart, 0x5000, 0x00);
        assert!(!mapper.irq_pending(), "acknowledged");
    }

    /// Each of the twelve 1K slots picks CHR-ROM or CIRAM independently. Values >= $E0 in a
    /// nametable register select CIRAM; anything lower is a CHR-ROM bank, which is how this board
    /// puts pattern data behind $2000.
    #[test]
    fn nametable_slots_select_ciram_or_chr_rom_per_register() {
        let (mut mapper, mut cart) = load();

        // $C000/$C800 are slots 8 and 9, i.e. $2000 and $2400.
        write(&mut mapper, &mut cart, 0xC000, 0xE0);
        write(&mut mapper, &mut cart, 0xC800, 0xE1);
        cart.memory.chr_write(0x2000, 0x11);
        cart.memory.chr_write(0x2400, 0x22);
        assert_eq!(chr_peek(&mapper, &cart, 0x2000), 0x11, "CIRAM page 0");
        assert_eq!(chr_peek(&mapper, &cart, 0x2400), 0x22, "CIRAM page 1");
        assert_eq!(
            chr_peek(&mapper, &cart, 0x3000),
            0x11,
            "$3000 mirrors $2000"
        );

        // Below $E0 the same slot serves CHR-ROM instead.
        write(&mut mapper, &mut cart, 0xC000, 16);
        assert_eq!(chr_peek(&mapper, &cart, 0x2000), 0x80 | 16, "CHR-ROM bank 16");
    }

    /// The eight CHR registers at $8000-$BFFF cover $0000-$1FFF in 1K slots.
    #[test]
    fn chr_registers_map_eight_1k_pattern_slots() {
        let (mut mapper, mut cart) = load();
        for slot in 0..8u16 {
            write(&mut mapper, &mut cart, 0x8000 + slot * 0x800, 20 + slot as u8);
        }
        for slot in 0..8u16 {
            assert_eq!(
                chr_peek(&mapper, &cart, slot * 1024),
                0x80 | (20 + slot as u8),
                "slot {slot}"
            );
        }
    }

    /// The 128-byte sound RAM is addressed indirectly through $F800 and read back through $4800,
    /// with optional auto-increment. It is battery-backed, so it also has to survive a save.
    #[test]
    fn sound_ram_is_addressed_indirectly_with_auto_increment() {
        let (mut mapper, mut cart) = load();

        // $F800 sets the pointer; bit 7 enables auto-increment.
        write(&mut mapper, &mut cart, 0xF800, 0x80 | 0x10);
        for val in 1..=4u8 {
            write(&mut mapper, &mut cart, 0x4800, val);
        }

        write(&mut mapper, &mut cart, 0xF800, 0x10);
        assert_eq!(prg_peek(&mapper, &cart, 0x4800), 1, "no auto-increment now");
        assert_eq!(prg_peek(&mapper, &cart, 0x4800), 1, "so it stays put");

        write(&mut mapper, &mut cart, 0xF800, 0x80 | 0x10);
        for val in 1..=4u8 {
            assert_eq!(
                mapper.prg_read(0x4800),
                Some(val),
                "auto-increment walks the four bytes written"
            );
        }
    }

    /// PRG-RAM is mapped and writable on a Namco163.
    #[test]
    fn prg_ram_is_mapped_and_writable() {
        let (mut mapper, mut cart) = load();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A);
        cart.memory.prg_write(0x6000, 0x77);
        mapper.write_register(&mut cart.memory, 0x6000, 0x77);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x77);
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state - page tables are never serialized.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xE000, 3);
        write(&mut mapper, &mut cart, 0xE800, 5);
        write(&mut mapper, &mut cart, 0xF000, 7);
        write(&mut mapper, &mut cart, 0x8000, 9);
        write(&mut mapper, &mut cart, 0xC000, 0xE1);

        let sample = |mapper: &Mapper, cart: &Cart| -> Vec<u8> {
            [0x6000, 0x8000, 0xA000, 0xC000, 0xE000]
                .into_iter()
                .map(|addr| prg_peek(mapper, cart, addr))
                .chain(
                    [0x0000, 0x2000, 0x3000]
                        .into_iter()
                        .map(|addr| chr_peek(mapper, cart, addr)),
                )
                .collect()
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
    }
}
