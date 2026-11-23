//! `Tengen RAMBO-1` (Mapper 064).
//!
//! <https://www.nesdev.org/wiki/RAMBO-1>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `Tengen RAMBO-1` (Mapper 064).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Rambo1 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// `$8000`: the register the next `$8001` write updates, plus the mode bits.
    pub bank_select: u8,
    /// `R0-RF`, written through `$8001`. `RA-RE` exist but bank nothing.
    pub regs: [u8; 16],
    /// `$C000`: the value the IRQ counter reloads from.
    pub irq_latch: u8,
    /// The IRQ counter.
    pub irq_counter: u8,
    /// Set by a `$C001` write, so the next counter clock reloads it.
    pub irq_reload: bool,
    /// `$C001` bit 0: the counter clocks every 4 CPU cycles instead of on PPU A12 rises.
    pub irq_cycle_mode: bool,
    /// The CPU-cycle prescaler, which clocks the counter each time it wraps to 0.
    pub irq_prescaler: u8,
    /// Set by a `$C001` write leaving cycle mode, so the prescaler clocks the counter once more.
    pub irq_force_clock: bool,
    /// `$E001` sets it and `$E000` clears it.
    pub irq_enabled: bool,
    /// CPU cycles left before a counter that reached 0 asserts the IRQ.
    pub irq_delay: u8,
    /// The IRQ line.
    pub irq_pending: bool,
    /// CPU cycles since power-on, for timing A12 rises.
    pub master_clock: u32,
    /// The CPU cycle A12 last went low, or 0 while it is high.
    pub a12_low_clock: u32,
}

impl Rambo1 {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    const CHR_INVERSION: u8 = 0x80;
    const PRG_MODE: u8 = 0x40;
    const CHR_1K_MODE: u8 = 0x20;
    // The RAMBO-1 IRQ arrives a few dots after MMC3's. Both delays are in CPU cycles. In A12 mode,
    // Klax's playfield bands jitter at 2 or less, and Rolling Thunder's status-bar split tears at
    // 5 or more. Mesen uses 2, and the A12 edge here is seen one cycle earlier than Mesen's.
    const A12_IRQ_DELAY: u8 = 3;
    const CYCLE_IRQ_DELAY: u8 = 1;

    // PPU $0000..=$1FFF 1K or 2K switchable CHR-ROM banks, in four modes
    // CPU $6000..=$7FFF 8K PRG-RAM
    // CPU $8000..=$DFFF three 8K switchable PRG-ROM banks, in two arrangements
    // CPU $E000..=$FFFF 8K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            bank_select: 0,
            regs: [0; 16],
            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_cycle_mode: false,
            irq_prescaler: 0,
            irq_force_clock: false,
            irq_enabled: false,
            irq_delay: 0,
            irq_pending: false,
            master_clock: 0,
            a12_low_clock: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    fn update_prg_banks(&self, memory: &mut Memory) {
        let [r6, r7, rf] = [6, 7, 15].map(|reg| i32::from(self.regs[reg]));
        let banks = if self.bank_select & Self::PRG_MODE == Self::PRG_MODE {
            [rf, r6, r7]
        } else {
            [r6, r7, rf]
        };
        for (slot, bank) in banks.into_iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, bank, Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
    }

    fn update_chr_banks(&self, memory: &mut Memory) {
        let r = |reg: usize| i32::from(self.regs[reg]);
        // The 1K banks for the half R0/R1 drive. In 2K mode, R0 and R1 bank pairs of pages.
        let pair_half = if self.bank_select & Self::CHR_1K_MODE == Self::CHR_1K_MODE {
            [r(0), r(8), r(1), r(9)]
        } else {
            let (r0, r1) = (r(0) & !1, r(1) & !1);
            [r0, r0 + 1, r1, r1 + 1]
        };
        let single_half = [r(2), r(3), r(4), r(5)];
        let (lo, hi) = if self.bank_select & Self::CHR_INVERSION == Self::CHR_INVERSION {
            (single_half, pair_half)
        } else {
            (pair_half, single_half)
        };
        for (slot, bank) in lo.into_iter().chain(hi).enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, bank, Src::Chr);
        }
    }

    /// Clock the IRQ counter, asserting the IRQ `delay` CPU cycles later if it reaches 0.
    const fn clock_irq_counter(&mut self, delay: u8) {
        if self.irq_reload {
            // Hard Drivin' needs the reload to land one higher for a latch above 1.
            self.irq_counter = if self.irq_latch <= 1 {
                self.irq_latch + 1
            } else {
                self.irq_latch.wrapping_add(2)
            };
            self.irq_reload = false;
        } else if self.irq_counter == 0 {
            self.irq_counter = self.irq_latch.wrapping_add(1);
        }
        self.irq_counter = self.irq_counter.wrapping_sub(1);
        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_delay = delay;
        }
    }

    // Matches `Mmc3::clock_irq`: A12 must stay low for a few CPU cycles before a rise counts, which
    // filters out the sprite-fetch toggles within one scanline.
    const fn is_a12_rising_edge(&mut self, addr: u16) -> bool {
        if addr & 0x1000 > 0 {
            let is_rising_edge =
                self.a12_low_clock > 0 && self.master_clock.wrapping_sub(self.a12_low_clock) >= 4;
            self.a12_low_clock = 0;
            return is_rising_edge;
        } else if self.a12_low_clock == 0 {
            self.a12_low_clock = self.master_clock;
        }
        false
    }
}

impl Map for Rambo1 {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ | MapperOps::WATCHES_PPU_BUS
    }

    fn ppu_bus_addr(&mut self, _memory: &mut Memory, addr: u16) {
        if self.is_a12_rising_edge(addr) && !self.irq_cycle_mode {
            self.clock_irq_counter(Self::A12_IRQ_DELAY);
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn set_irq_pending(&mut self, pending: bool) {
        self.irq_pending = pending;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        const NAMES: [&str; 16] = [
            "R0", "R1", "R2", "R3", "R4", "R5", "R6", "R7", "R8", "R9", "RA", "RB", "RC", "RD",
            "RE", "RF",
        ];
        out.push(("Bank select", u32::from(self.bank_select)));
        for (name, reg) in NAMES.into_iter().zip(self.regs) {
            out.push((name, u32::from(reg)));
        }
        out.push(("IRQ latch", u32::from(self.irq_latch)));
        out.push(("IRQ counter", u32::from(self.irq_counter)));
        out.push(("IRQ cycle mode", u32::from(self.irq_cycle_mode)));
        out.push(("IRQ enabled", u32::from(self.irq_enabled)));
        out.push(("IRQ pending", u32::from(self.irq_pending)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        match addr & 0xE001 {
            0x8000 => {
                self.bank_select = val;
                self.update_banks(memory);
            }
            0x8001 => {
                self.regs[usize::from(self.bank_select & 0x0F)] = val;
                self.update_banks(memory);
            }
            0xA000 => {
                // A four-screen cart has its own nametable RAM and ignores the mirroring bit.
                if self.mirroring != Mirroring::FourScreen {
                    self.mirroring = if val & 0x01 == 0x01 {
                        Mirroring::Horizontal
                    } else {
                        Mirroring::Vertical
                    };
                    memory.set_mirroring(self.mirroring);
                }
            }
            0xC000 => self.irq_latch = val,
            0xC001 => {
                let cycle_mode = val & 0x01 == 0x01;
                // Skull & Crossbones leaves cycle mode every frame. The switch takes effect only
                // after the prescaler's next wrap, so the counter gets one more cycle-mode clock.
                // This follows Mesen, which credits FHorse.
                if self.irq_cycle_mode && !cycle_mode {
                    self.irq_force_clock = true;
                }
                if cycle_mode {
                    self.irq_prescaler = 0;
                }
                self.irq_cycle_mode = cycle_mode;
                self.irq_reload = true;
            }
            0xE000 => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0xE001 => self.irq_enabled = true,
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, Self::PRG_WINDOW, 0, Src::PrgRam);
        self.update_prg_banks(memory);
        self.update_chr_banks(memory);
        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        self.master_clock = self.master_clock.wrapping_add(1);
        if self.irq_delay > 0 {
            self.irq_delay -= 1;
            if self.irq_delay == 0 {
                self.irq_pending = true;
            }
        }
        if self.irq_cycle_mode || self.irq_force_clock {
            self.irq_prescaler = (self.irq_prescaler + 1) & 0x03;
            if self.irq_prescaler == 0 {
                self.clock_irq_counter(Self::CYCLE_IRQ_DELAY);
                self.irq_force_clock = false;
            }
        }
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.bank_select = 0;
        self.regs = [0; 16];
        self.irq_latch = 0;
        self.irq_counter = 0;
        self.irq_reload = false;
        self.irq_cycle_mode = false;
        self.irq_prescaler = 0;
        self.irq_force_clock = false;
        self.irq_enabled = false;
        self.irq_delay = 0;
        self.irq_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 64K PRG-ROM (8 8K banks), 8K PRG-RAM and 64K CHR-ROM.
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(64 * 1024, 8 * 1024, 64 * 1024);
        let mapper = Rambo1::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Writes `val` to bank register `reg`, with `bank_select`'s mode bits.
    fn bank(mapper: &mut Mapper, cart: &mut Cart, mode: u8, reg: u8, val: u8) {
        write(mapper, cart, 0x8000, mode | reg);
        write(mapper, cart, 0x8001, val);
    }

    fn chr_pages(mapper: &Mapper, cart: &Cart) -> [u8; 8] {
        [0, 1, 2, 3, 4, 5, 6, 7].map(|page| chr_peek(mapper, cart, page * 0x400) & 0x7F)
    }

    /// RF is the third PRG register, and the PRG mode bit rotates it to `$8000`.
    #[test]
    fn prg_mode_rotates_rf_to_8000() {
        let (mut mapper, mut cart) = load();
        bank(&mut mapper, &mut cart, 0x00, 6, 1);
        bank(&mut mapper, &mut cart, 0x00, 7, 2);
        bank(&mut mapper, &mut cart, 0x00, 15, 3);
        let prg =
            |m: &Mapper, c: &Cart| [0x8000, 0xA000, 0xC000, 0xE000].map(|a| prg_peek(m, c, a));
        assert_eq!(prg(&mapper, &cart), [8, 16, 24, 56]);

        write(&mut mapper, &mut cart, 0x8000, 0x40);
        assert_eq!(prg(&mapper, &cart), [24, 8, 16, 56]);
    }

    /// The four CHR modes: 2K or 1K banking for R0/R1, each with or without inversion.
    #[test]
    fn chr_modes() {
        let (mut mapper, mut cart) = load();
        for (reg, val) in [
            (0, 0x11),
            (1, 0x21),
            (2, 2),
            (3, 3),
            (4, 4),
            (5, 5),
            (8, 8),
            (9, 9),
        ] {
            bank(&mut mapper, &mut cart, 0x00, reg, val);
        }
        let cases = [
            (0x00, [0x10, 0x11, 0x20, 0x21, 2, 3, 4, 5]),
            (0x20, [0x11, 8, 0x21, 9, 2, 3, 4, 5]),
            (0x80, [2, 3, 4, 5, 0x10, 0x11, 0x20, 0x21]),
            (0xA0, [2, 3, 4, 5, 0x11, 8, 0x21, 9]),
        ];
        for (mode, pages) in cases {
            write(&mut mapper, &mut cart, 0x8000, mode);
            assert_eq!(chr_pages(&mapper, &cart), pages, "mode {mode:#04X}");
        }
    }

    /// In cycle mode the counter clocks every 4 CPU cycles, reloads one higher after a `$C001`
    /// write, and the IRQ lands one CPU cycle after it reaches 0.
    #[test]
    fn cycle_mode_irq() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xC000, 3);
        write(&mut mapper, &mut cart, 0xC001, 0x01);
        write(&mut mapper, &mut cart, 0xE001, 0);
        // Reload to 3 + 2 then count down to 0 takes 5 counter clocks of 4 CPU cycles each.
        for _ in 0..20 {
            assert!(!mapper.irq_pending());
            mapper.clock();
        }
        assert!(!mapper.irq_pending(), "delayed one cycle");
        mapper.clock();
        assert!(mapper.irq_pending());

        write(&mut mapper, &mut cart, 0xE000, 0);
        assert!(!mapper.irq_pending(), "acknowledged");
    }

    /// Leaving cycle mode lets the prescaler run to its next wrap and clock the counter once more.
    #[test]
    fn leaving_cycle_mode_clocks_the_counter_once_more() {
        let counter = |mapper: &Mapper| match mapper {
            Mapper::Rambo1(board) => board.irq_counter,
            _ => unreachable!("loaded as RAMBO-1"),
        };
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xC000, 3);
        write(&mut mapper, &mut cart, 0xC001, 0x01);
        mapper.clock();
        mapper.clock();
        write(&mut mapper, &mut cart, 0xC001, 0x00);
        mapper.clock();
        assert_eq!(counter(&mapper), 0, "prescaler has not wrapped");
        mapper.clock();
        assert_eq!(counter(&mapper), 4, "reloaded to 3 + 2, then clocked");
        for _ in 0..16 {
            mapper.clock();
        }
        assert_eq!(counter(&mapper), 4, "back in A12 mode");
    }
}
