//! `Sunsoft FME7` (Mapper 069).
//!
//! <https://www.nesdev.org/wiki/Sunsoft_FME-7>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    apu::PULSE_TABLE,
    cart::Cart,
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
    /// Page selected into the $6000 window, from PRG-RAM or PRG-ROM per R:8's RAM/ROM select.
    pub prg_ram_bank: u8,
}

impl SunsoftFme7 {
    const PRG_WINDOW: usize = 8 * 1024;
    /// R:8 bit 6: RAM/ROM select for the $6000 window. Bit 7 only enables the RAM.
    const PRG_RAM_SELECT: u8 = 0x40;
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
        board.update_banks(&mut cart.memory);
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
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        for (slot, bank) in self.chr_banks.iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*bank), Src::Chr);
        }
        // R:8 is [ERPP PPPP] (`docs/mapper/069.txt:55`): R picks RAM or ROM for the $6000 window
        // and E only enables the RAM, so selecting RAM while it is disabled is neither - the
        // window is open bus. Reading E as the select instead makes ROM unreachable whenever RAM
        // is enabled, and open bus unreachable altogether.
        let bank = i32::from(self.prg_ram_bank);
        if self.regs.parameter & Self::PRG_RAM_SELECT == 0 {
            memory.map_prg(0x6000, Self::PRG_WINDOW, bank, Src::PrgRom);
        } else if self.regs.prg_ram_enabled {
            memory.map_prg(0x6000, Self::PRG_WINDOW, bank, Src::PrgRam);
        } else {
            memory.unmap_prg(0x6000, Self::PRG_WINDOW);
        }
        for (slot, bank) in self.prg_banks.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, i32::from(*bank), Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        if self.regs.irq_counter_enabled {
            self.regs.irq_counter = self.regs.irq_counter.wrapping_sub(1);
            if self.regs.irq_counter == 0xFFFF && self.regs.irq_enabled {
                self.regs.irq_pending = true;
            }
        }
        self.audio.clock();
    }

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
    pub fn clock(&mut self) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (128 1K pages), 8K PRG-RAM, 64K CHR-ROM (64 1K pages).
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 64 * 1024);
        let mapper = SunsoftFme7::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Every register is a command/parameter pair: $8000 picks the command, $A000 supplies its
    /// value.
    fn command(mapper: &mut Mapper, cart: &mut Cart, command: u8, parameter: u8) {
        write(mapper, cart, 0x8000, command);
        write(mapper, cart, 0xA000, parameter);
    }

    /// Commands 9-$B are the three switchable 8K windows; $E000 is fixed to the last bank.
    #[test]
    fn prg_windows_are_three_switchable_8k_banks_and_a_fixed_last() {
        let (mut mapper, mut cart) = load();
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120, "last bank at $E000");

        command(&mut mapper, &mut cart, 0x9, 3);
        command(&mut mapper, &mut cart, 0xA, 5);
        command(&mut mapper, &mut cart, 0xB, 7);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 5 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120, "still fixed");
    }

    /// Commands 0-7 are the eight 1K CHR windows.
    #[test]
    fn chr_commands_map_eight_1k_banks() {
        let (mut mapper, mut cart) = load();
        for slot in 0..8u8 {
            command(&mut mapper, &mut cart, slot, 20 + slot);
        }
        for slot in 0..8u16 {
            assert_eq!(
                chr_peek(&mapper, &cart, slot * 1024),
                0x80 | (20 + slot as u8),
                "slot {slot}"
            );
        }
    }

    /// Command 8 is the board's distinctive one: `[ERPP PPPP]`, where R picks RAM or ROM for the
    /// $6000 window and E only enables the RAM. The same page index therefore means two different
    /// things, and asking for RAM that is disabled means neither.
    ///
    /// Every FME-7 cart writes E and R together - `$00`, `$C0`, or a bare ROM page - so the two
    /// mixed encodings are unreachable in practice and reading E as the select gives the right
    /// answer for all of them. The decode still has to be right: nothing stops a cart from
    /// mapping ROM at $6000 while its RAM is enabled.
    #[test]
    fn the_6000_window_selects_prg_ram_or_prg_rom_per_the_select_bit() {
        let (mut mapper, mut cart) = load();

        // R clear: PRG-ROM page 5, an ordinary 8K window into the ROM.
        command(&mut mapper, &mut cart, 0x8, 5);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 5 * 8, "PRG-ROM page 5");

        // R and E set: the same index now selects PRG-RAM.
        command(&mut mapper, &mut cart, 0x8, 0xC0);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "PRG-RAM");

        // R set with E clear selects RAM that is not enabled, which is open bus.
        command(&mut mapper, &mut cart, 0x8, 0x40);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0, "open bus");

        // E set with R clear is still ROM - the enable does not select anything.
        command(&mut mapper, &mut cart, 0x8, 0x80 | 5);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x6000),
            5 * 8,
            "PRG-ROM page 5 again"
        );
    }

    /// Command $C sets mirroring, which for this board is ordinary CIRAM mirroring.
    #[test]
    fn command_c_sets_mirroring() {
        let (mut mapper, mut cart) = load();

        command(&mut mapper, &mut cart, 0xC, 0b00);
        cart.memory.chr_write(0x2000, 0x11);
        cart.memory.chr_write(0x2400, 0x22);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x2800),
            0x11,
            "vertical: $2800=$2000"
        );

        command(&mut mapper, &mut cart, 0xC, 0b01);
        cart.memory.chr_write(0x2000, 0x33);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x2400),
            0x33,
            "horizontal: $2400=$2000"
        );

        command(&mut mapper, &mut cart, 0xC, 0b10);
        cart.memory.chr_write(0x2000, 0x44);
        for nt in [0x2400, 0x2800, 0x2C00] {
            assert_eq!(chr_peek(&mapper, &cart, nt), 0x44, "single screen A");
        }
    }

    /// The IRQ counter is a 16-bit *down* counter that fires on the wrap past zero, so a value of
    /// `n` gives `n + 1` clocks.
    #[test]
    fn irq_counter_counts_down_and_fires_on_the_wrap_past_zero() {
        let (mut mapper, mut cart) = load();
        command(&mut mapper, &mut cart, 0xE, 10); // counter low
        command(&mut mapper, &mut cart, 0xF, 0); // counter high
        command(&mut mapper, &mut cart, 0xD, 0x81); // count, and interrupt

        for clock in 0..10 {
            mapper.clock();
            assert!(!mapper.irq_pending(), "too early, at clock {clock}");
        }
        mapper.clock();
        assert!(mapper.irq_pending(), "fires as the counter wraps past zero");
    }

    /// Bit 7 enables counting and bit 0 enables the interrupt - they are separate, so the counter
    /// can run without ever asserting.
    #[test]
    fn counting_and_interrupting_are_separately_enabled() {
        let (mut mapper, mut cart) = load();
        command(&mut mapper, &mut cart, 0xE, 5);
        command(&mut mapper, &mut cart, 0xF, 0);

        // Counting enabled, interrupt disabled.
        command(&mut mapper, &mut cart, 0xD, 0x80);
        for _ in 0..100 {
            mapper.clock();
        }
        assert!(!mapper.irq_pending(), "counts, but never asserts");

        // Interrupt enabled, counting disabled.
        command(&mut mapper, &mut cart, 0xE, 1);
        command(&mut mapper, &mut cart, 0xF, 0);
        command(&mut mapper, &mut cart, 0xD, 0x01);
        for _ in 0..100 {
            mapper.clock();
        }
        assert!(!mapper.irq_pending(), "a frozen counter never reaches zero");
    }

    /// Writing the control register acknowledges a pending IRQ.
    #[test]
    fn writing_the_control_register_acknowledges_the_irq() {
        let (mut mapper, mut cart) = load();
        command(&mut mapper, &mut cart, 0xE, 0);
        command(&mut mapper, &mut cart, 0xF, 0);
        command(&mut mapper, &mut cart, 0xD, 0x81);
        mapper.clock();
        assert!(mapper.irq_pending());

        command(&mut mapper, &mut cart, 0xD, 0x81);
        assert!(!mapper.irq_pending(), "acknowledged");
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = load();
        command(&mut mapper, &mut cart, 0x8, 0x80);
        command(&mut mapper, &mut cart, 0x9, 3);
        command(&mut mapper, &mut cart, 0xA, 5);
        command(&mut mapper, &mut cart, 0xB, 7);
        command(&mut mapper, &mut cart, 0x0, 9);

        let sample = |mapper: &Mapper, cart: &Cart| -> Vec<u8> {
            [0x6000, 0x8000, 0xA000, 0xC000, 0xE000]
                .into_iter()
                .map(|addr| prg_peek(mapper, cart, addr))
                .chain([chr_peek(mapper, cart, 0x0000)])
                .collect()
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
    }
}
