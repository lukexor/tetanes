//! `VRC6` (Mapper 024).
//!
//! <https://www.nesdev.org/wiki/VRC6>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    apu::PULSE_TABLE,
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps, vrc_irq::VrcIrq},
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
        board.update_banks(&mut cart.memory);
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
    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("Banking mode", u32::from(self.regs.banking_mode)));
        for (slot, bank) in self.prg_banks.iter().enumerate() {
            out.push((["PRG $8000 (16K)", "PRG $C000"][slot], *bank as u32));
        }
        for (slot, bank) in self.regs.chr.iter().enumerate() {
            out.push((
                [
                    "CHR 0", "CHR 1", "CHR 2", "CHR 3", "CHR 4", "CHR 5", "CHR 6", "CHR 7",
                ][slot],
                *bank as u32,
            ));
        }
        out.push(("IRQ reload", u32::from(self.irq.reload)));
        out.push(("IRQ counter", u32::from(self.irq.counter)));
        out.push(("IRQ enabled", u32::from(self.irq.enabled)));
        out.push(("IRQ pending", u32::from(self.irq.irq_pending)));
        out.push(("IRQ cycle mode", u32::from(self.irq.cycle_mode)));
    }
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ | MapperOps::AUDIO
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
            // The IRQ registers drive the scanline counter and move nothing that is banked, so a
            // game arming them per scanline does not rebuild all 40 page entries.
            0xF000 => {
                self.irq.write_reload(val);
                return;
            }
            0xF001 => {
                self.irq.write_control(val);
                return;
            }
            0xF002 => {
                self.irq.acknowledge();
                return;
            }
            _ => return,
        }
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
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

    fn clock(&mut self) {
        self.irq.clock();
        self.audio.clock();
    }

    fn reset(&mut self, kind: ResetKind) {
        self.irq.reset(kind);
        self.audio.reset(kind);
    }

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
    pub fn clock(&mut self) {
        if !self.halt {
            self.pulse1.clock();
            self.pulse2.clock();
            self.saw.clock();

            self.out = self.pulse1.volume() + self.pulse2.volume() + self.saw.volume();
        }
    }
    pub const fn reset(&mut self, _kind: ResetKind) {
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
    pub const fn clock(&mut self) {
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
    pub const fn clock(&mut self) {
        if self.enabled {
            self.timer -= 1;
            if self.timer == 0 {
                self.step = (self.step + 1) % 14;
                self.timer = (self.frequency >> self.freq_shift) + 1;

                if self.step == 0 {
                    self.accum = 0;
                } else if self.step & 0x01 == 0x00 {
                    // The accumulator is 8 bits and the hardware simply discards the carry. Six
                    // accumulations of a rate above 42 exceed 255, so this must wrap rather than
                    // panic a debug build.
                    self.accum = self.accum.wrapping_add(self.accum_rate);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (128 1K pages), 8K PRG-RAM, 64K CHR-ROM (64 1K pages).
    fn load(revision: Revision) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 64 * 1024);
        let mapper = Vrc6::load(&mut cart, revision).expect("valid mapper");
        (mapper, cart)
    }

    fn vrc6a() -> (Mapper, Cart) {
        load(Revision::A)
    }

    fn audio(mapper: &mut Mapper) -> &mut Audio {
        match mapper {
            Mapper::Vrc6(vrc6) => &mut vrc6.audio,
            _ => unreachable!("mapper is a Vrc6"),
        }
    }

    /// $E000 is hard-wired to the last 8K, which is where the reset vector lives - if this is wrong
    /// nothing boots at all.
    #[test]
    fn powers_on_with_the_last_prg_bank_fixed_at_e000() {
        let (mapper, cart) = vrc6a();
        // 128K in 1K pages is 128; the last 8K window starts at page 120.
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120);
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 127);
    }

    /// $8000 is a 16K window and $C000 an 8K one, so the same bank number means a different page in
    /// each - the easiest thing to get wrong when moving to a page table.
    #[test]
    fn prg_windows_are_16k_at_8000_and_8k_at_c000() {
        let (mut mapper, mut cart) = vrc6a();

        write(&mut mapper, &mut cart, 0x8000, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "16K bank 3");
        assert_eq!(
            prg_peek(&mapper, &cart, 0xA000),
            3 * 16 + 8,
            "the window really is 16K, so $A000 is 8K into it"
        );

        write(&mut mapper, &mut cart, 0xC000, 5);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 5 * 8, "8K bank 5");

        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120, "$E000 stays fixed");
    }

    /// Bit 7 of the banking mode gates PRG-RAM; with it clear the window must read as unmapped
    /// rather than as whatever was there before.
    #[test]
    fn prg_ram_at_6000_follows_the_banking_mode_bit() {
        let (mut mapper, mut cart) = vrc6a();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0, "disabled at power-on");

        write(&mut mapper, &mut cart, 0xB003, 0x80);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "enabled");

        write(&mut mapper, &mut cart, 0xB003, 0x00);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0, "disabled again");
    }

    /// Banking mode 0 is eight independent 1K banks, one per register.
    #[test]
    fn chr_mode_0_maps_eight_independent_1k_banks() {
        let (mut mapper, mut cart) = vrc6a();
        write(&mut mapper, &mut cart, 0xB003, 0x00);
        for reg in 0..4 {
            write(&mut mapper, &mut cart, 0xD000 + reg, 1 + reg as u8);
            write(&mut mapper, &mut cart, 0xE000 + reg, 5 + reg as u8);
        }
        for (slot, bank) in [1, 2, 3, 4, 5, 6, 7, 8].into_iter().enumerate() {
            let addr = (slot * 1024) as u16;
            assert_eq!(chr_peek(&mapper, &cart, addr), 0x80 | bank, "slot {slot}");
        }
    }

    /// Banking mode 1 is four 2K banks: each register covers two consecutive 1K slots.
    #[test]
    fn chr_mode_1_maps_four_2k_banks() {
        let (mut mapper, mut cart) = vrc6a();
        write(&mut mapper, &mut cart, 0xB003, 0x01);
        for reg in 0..4 {
            write(&mut mapper, &mut cart, 0xD000 + reg, 10 + reg as u8);
        }
        // Bit 5 is clear, so a register's low bit is used as-is and both halves of the pair select
        // the same 1K bank.
        for (slot, bank) in [10, 10, 11, 11, 12, 12, 13, 13].into_iter().enumerate() {
            let addr = (slot * 1024) as u16;
            assert_eq!(chr_peek(&mapper, &cart, addr), 0x80 | bank, "slot {slot}");
        }
    }

    /// Bit 5 aligns the pairs, so a 2K bank really is two consecutive 1K banks.
    #[test]
    fn chr_bank_pairs_align_when_bit_5_is_set() {
        let (mut mapper, mut cart) = vrc6a();
        write(&mut mapper, &mut cart, 0xB003, 0x21);
        for reg in 0..4 {
            // Odd values, to prove the low bit is being masked off rather than ignored.
            write(&mut mapper, &mut cart, 0xD000 + reg, 11 + 2 * reg as u8);
        }
        for (slot, bank) in [10, 11, 12, 13, 14, 15, 16, 17].into_iter().enumerate() {
            let addr = (slot * 1024) as u16;
            assert_eq!(chr_peek(&mapper, &cart, addr), 0x80 | bank, "slot {slot}");
        }
    }

    /// Bit 4 switches the nametables from CIRAM to CHR-ROM. The rework made these ordinary page
    /// entries, which is what removed the special case from the PPU read path - so this is the
    /// regression test for that change.
    #[test]
    fn nametables_come_from_ciram_or_chr_rom_per_bit_4() {
        let (mut mapper, mut cart) = vrc6a();

        // Bit 4 clear: CIRAM, and writable, so a write-then-read round trips.
        write(&mut mapper, &mut cart, 0xB003, 0x00);
        cart.memory.chr_write(0x2000, 0x11);
        assert_eq!(chr_peek(&mapper, &cart, 0x2000), 0x11, "CIRAM is writable");

        // Bit 4 set: banking mode $10 selects [chr6, chr6, chr7, chr7] out of CHR-ROM.
        write(&mut mapper, &mut cart, 0xE002, 10);
        write(&mut mapper, &mut cart, 0xE003, 20);
        write(&mut mapper, &mut cart, 0xB003, 0x10);
        for (slot, bank) in [10, 10, 20, 20].into_iter().enumerate() {
            let addr = 0x2000 + (slot * 1024) as u16;
            assert_eq!(chr_peek(&mapper, &cart, addr), 0x80 | bank, "slot {slot}");
            assert_eq!(
                chr_peek(&mapper, &cart, addr + 0x1000),
                0x80 | bank,
                "$3000 mirrors $2000, slot {slot}"
            );
        }
    }

    /// VRC6b is the same board with A0 and A1 swapped, so $8001 on a VRC6b is $8002 on a VRC6a.
    /// Getting this backwards silently sends bank writes to the audio registers.
    #[test]
    fn vrc6b_swaps_the_low_two_address_lines() {
        let (mut a, mut cart_a) = load(Revision::A);
        let (mut b, mut cart_b) = load(Revision::B);

        // $D001 selects chr[1] on a VRC6a; on a VRC6b the same wire is $D002.
        write(&mut a, &mut cart_a, 0xD001, 7);
        write(&mut b, &mut cart_b, 0xD002, 7);
        assert_eq!(chr_peek(&a, &cart_a, 0x0400), 0x80 | 7);
        assert_eq!(
            chr_peek(&b, &cart_b, 0x0400),
            0x80 | 7,
            "VRC6b reaches chr[1] through $D002"
        );

        // $B003 is unaffected: both low bits are set, so the swap is a no-op.
        write(&mut b, &mut cart_b, 0xB003, 0x80);
        assert_eq!(prg_peek(&b, &cart_b, 0x6000), 0x5A);
    }

    /// `update_banks` has to rebuild every window from the register state alone - that is what
    /// `Ppu::rebuild_mapper_state` calls after loading a save state, which carries no page tables.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = vrc6a();
        write(&mut mapper, &mut cart, 0x8000, 3);
        write(&mut mapper, &mut cart, 0xC000, 5);
        write(&mut mapper, &mut cart, 0xB003, 0x80);
        write(&mut mapper, &mut cart, 0xD000, 9);

        let before: Vec<u8> = [0x6000, 0x8000, 0xA000, 0xC000, 0xE000]
            .into_iter()
            .map(|addr| prg_peek(&mapper, &cart, addr))
            .chain(
                [0x0000, 0x2000]
                    .into_iter()
                    .map(|addr| chr_peek(&mapper, &cart, addr)),
            )
            .collect();

        // Wipe every mapping, then rebuild from the registers alone.
        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        let after: Vec<u8> = [0x6000, 0x8000, 0xA000, 0xC000, 0xE000]
            .into_iter()
            .map(|addr| prg_peek(&mapper, &cart, addr))
            .chain(
                [0x0000, 0x2000]
                    .into_iter()
                    .map(|addr| chr_peek(&mapper, &cart, addr)),
            )
            .collect();
        assert_eq!(before, after);
    }

    /// A channel is silent until its enable bit is set, and `halt` stops the whole audio unit.
    #[test]
    fn pulse_is_silent_until_enabled_and_halt_freezes_it() {
        let (mut mapper, _cart) = vrc6a();
        let audio = audio(&mut mapper);

        // Volume 15, duty ignored, so the only gate left is the enable bit.
        audio.write_register(0x9000, 0x8F);
        audio.clock();
        assert_eq!(audio.output(), 0.0, "silent while disabled");

        audio.write_register(0x9002, 0x80);
        audio.clock();
        let running = audio.output();
        assert!(running > 0.0, "audible once enabled, got {running}");

        // Halt stops the clock, so the output freezes where it was rather than resetting.
        audio.write_register(0x9003, 0x01);
        audio.write_register(0x9000, 0x80); // volume 0 - would silence it if it were still clocking
        audio.clock();
        assert_eq!(audio.output(), running, "halted, so the output is frozen");
    }

    /// The duty cycle gates the pulse, unless bit 7 of the volume register overrides it.
    #[test]
    fn duty_cycle_gates_the_pulse_unless_ignored() {
        let (mut mapper, _cart) = vrc6a();
        let audio = audio(&mut mapper);

        // Duty 0, volume 15, frequency 0 so the step advances every clock.
        audio.write_register(0x9000, 0x0F);
        audio.write_register(0x9001, 0x00);
        audio.write_register(0x9002, 0x80);

        // Only step 0 is within a duty of 0, and the step advances every clock, so exactly one of
        // the 16 steps is audible.
        let audible = (0..16)
            .filter(|_| {
                audio.clock();
                audio.output() > 0.0
            })
            .count();
        assert_eq!(audible, 1, "duty 0 is audible for one step in 16");

        audio.write_register(0x9000, 0x8F); // ignore duty
        let audible = (0..16)
            .filter(|_| {
                audio.clock();
                audio.output() > 0.0
            })
            .count();
        assert_eq!(audible, 16, "ignoring duty is audible on every step");
    }

    /// The saw accumulates its rate on alternate steps and resets every 14, and the accumulator is
    /// 8-bit: a rate above 42 overflows it, which the hardware simply wraps.
    #[test]
    fn saw_accumulates_and_wraps_its_8_bit_accumulator() {
        let (mut mapper, _cart) = vrc6a();
        let audio = audio(&mut mapper);

        // Maximum rate, frequency 0 so a step happens every clock.
        audio.write_register(0xB000, 0x3F);
        audio.write_register(0xB001, 0x00);
        audio.write_register(0xB002, 0x80);

        let mut outputs = Vec::new();
        for _ in 0..14 {
            audio.clock();
            outputs.push(audio.output());
        }
        assert!(
            outputs.iter().any(|&out| out > 0.0),
            "the saw must ramp: {outputs:?}"
        );
        assert_eq!(
            outputs[13], 0.0,
            "step 0 comes back around after 14 and resets the accumulator"
        );
    }
}
