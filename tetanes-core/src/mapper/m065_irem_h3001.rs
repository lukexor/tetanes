//! `Irem H3001` (Mapper 065).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_065>

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

/// `Irem H3001` (Mapper 065).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct IremH3001 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// `$8000`, `$A000` and `$C000`: 8K PRG banks.
    pub prg: [u8; 3],
    /// `$B000-$B007`: 1K CHR banks.
    pub chr: [u8; 8],
    /// `$9003` bit 7: the counter runs.
    pub irq_enabled: bool,
    /// The counter reached zero and the IRQ is not yet acknowledged.
    pub irq_pending: bool,
    /// `$9005` (high byte) and `$9006` (low byte): the value `$9004` copies into the counter.
    pub irq_reload: u16,
    /// The CPU-cycle down-counter.
    pub irq_counter: u16,
}

impl IremH3001 {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    // The PRG registers' power-on values. $FE is the second-last bank of a 256-bank space, and
    // the games crash without it.
    const PRG_POWER_ON: [u8; 3] = [0x00, 0x01, 0xFE];

    // PPU $0000..=$1FFF eight 1K switchable CHR-ROM banks
    // CPU $8000..=$DFFF three 8K switchable PRG-ROM banks
    // CPU $E000..=$FFFF 8K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg: Self::PRG_POWER_ON,
            chr: [0; 8],
            irq_enabled: false,
            irq_pending: false,
            irq_reload: 0,
            irq_counter: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    fn update_prg_banks(&self, memory: &mut Memory) {
        for (slot, bank) in self.prg.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, i32::from(*bank), Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
    }
}

impl Map for IremH3001 {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn set_irq_pending(&mut self, pending: bool) {
        self.irq_pending = pending;
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        for (name, bank) in ["PRG $8000", "PRG $A000", "PRG $C000"]
            .into_iter()
            .zip(self.prg)
        {
            out.push((name, u32::from(bank)));
        }
        let chr = [
            "CHR 0", "CHR 1", "CHR 2", "CHR 3", "CHR 4", "CHR 5", "CHR 6", "CHR 7",
        ];
        for (name, bank) in chr.into_iter().zip(self.chr) {
            out.push((name, u32::from(bank)));
        }
        out.push(("IRQ counter", u32::from(self.irq_counter)));
        out.push(("IRQ reload", u32::from(self.irq_reload)));
        out.push(("IRQ enabled", u32::from(self.irq_enabled)));
        out.push(("IRQ pending", u32::from(self.irq_pending)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr {
            0x8000 | 0xA000 | 0xC000 => {
                self.prg[usize::from((addr - 0x8000) >> 13)] = val;
                self.update_prg_banks(memory);
            }
            0x9001 => {
                self.mirroring = if val & 0x80 == 0x80 {
                    Mirroring::Horizontal
                } else {
                    Mirroring::Vertical
                };
                memory.set_mirroring(self.mirroring);
            }
            0x9003 => {
                self.irq_enabled = val & 0x80 == 0x80;
                self.irq_pending = false;
            }
            0x9004 => {
                self.irq_counter = self.irq_reload;
                self.irq_pending = false;
            }
            0x9005 => self.irq_reload = (self.irq_reload & 0x00FF) | (u16::from(val) << 8),
            0x9006 => self.irq_reload = (self.irq_reload & 0xFF00) | u16::from(val),
            0xB000..=0xB007 => {
                let slot = usize::from(addr & 0x07);
                self.chr[slot] = val;
                let addr = (slot * Self::CHR_WINDOW) as u16;
                memory.map_chr(addr, Self::CHR_WINDOW, i32::from(val), Src::Chr);
            }
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        self.update_prg_banks(memory);
        for (slot, bank) in self.chr.iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*bank), Src::Chr);
        }
        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        // The counter stops at zero rather than wrapping, so it fires once per reload.
        if self.irq_enabled && self.irq_counter > 0 {
            self.irq_counter -= 1;
            if self.irq_counter == 0 {
                self.irq_pending = true;
            }
        }
    }

    fn reset(&mut self, kind: ResetKind) {
        // The caller runs `update_banks` after reset.
        if kind == ResetKind::Hard {
            self.prg = Self::PRG_POWER_ON;
        }
        self.irq_enabled = false;
        self.irq_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (16 8K banks) and 64K CHR-ROM (64 1K banks).
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 64 * 1024);
        let mapper = IremH3001::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Power-on puts banks 0 and 1 at `$8000`/`$A000`, the second-last at `$C000` and the last at
    /// `$E000`.
    #[test]
    fn power_on_prg_layout() {
        let (mapper, cart) = load();
        for (addr, bank) in [(0x8000, 0), (0xA000, 1), (0xC000, 14), (0xE000, 15)] {
            assert_eq!(prg_peek(&mapper, &cart, addr), bank * 8, "${addr:04X}");
        }
    }

    /// Three switchable 8K PRG windows, eight 1K CHR windows, and a fixed last PRG bank.
    #[test]
    fn prg_and_chr_registers_bank_their_windows() {
        let (mut mapper, mut cart) = load();
        for (reg, bank) in [(0x8000, 5), (0xA000, 6), (0xC000, 7)] {
            write(&mut mapper, &mut cart, reg, bank);
            assert_eq!(prg_peek(&mapper, &cart, reg), bank * 8, "${reg:04X}");
        }
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 15 * 8, "fixed last bank");

        for slot in 0..8u16 {
            write(&mut mapper, &mut cart, 0xB000 + slot, 40 + slot as u8);
        }
        for slot in 0..8u16 {
            assert_eq!(
                chr_peek(&mapper, &cart, slot * 0x400),
                0x80 | (40 + slot as u8),
                "CHR slot {slot}"
            );
        }
    }

    /// `$9001` bit 7 selects horizontal mirroring, clear selects vertical.
    #[test]
    fn mirroring_register() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x9001, 0x80);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        write(&mut mapper, &mut cart, 0x9001, 0x00);
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    }

    /// `$9004` loads the counter from `$9005`/`$9006`, the IRQ fires after that many CPU cycles,
    /// and the counter then stays at zero.
    #[test]
    fn irq_counts_down_once_per_reload() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x9005, 0x01);
        write(&mut mapper, &mut cart, 0x9006, 0x02);
        write(&mut mapper, &mut cart, 0x9004, 0);
        write(&mut mapper, &mut cart, 0x9003, 0x80);

        for clock in 0..0x0101 {
            mapper.clock();
            assert!(!mapper.irq_pending(), "too early, at clock {clock}");
        }
        mapper.clock();
        assert!(mapper.irq_pending(), "fires after $0102 cycles");

        write(&mut mapper, &mut cart, 0x9003, 0x80);
        assert!(!mapper.irq_pending(), "$9003 acknowledges");
        for _ in 0..0x20000 {
            mapper.clock();
        }
        assert!(!mapper.irq_pending(), "the counter does not wrap");
    }

    /// Writing `$9004` acknowledges a pending IRQ, and a disabled counter never fires.
    #[test]
    fn reload_acknowledges_and_disable_stops_the_counter() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x9006, 0x01);
        write(&mut mapper, &mut cart, 0x9004, 0);
        write(&mut mapper, &mut cart, 0x9003, 0x80);
        mapper.clock();
        assert!(mapper.irq_pending());

        write(&mut mapper, &mut cart, 0x9004, 0);
        assert!(!mapper.irq_pending(), "reload acknowledges");

        write(&mut mapper, &mut cart, 0x9003, 0x00);
        for _ in 0..10 {
            mapper.clock();
        }
        assert!(!mapper.irq_pending(), "a disabled counter never fires");
    }
}
