//! `VRC3` (Mapper 073).
//!
//! <https://www.nesdev.org/wiki/VRC3>

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

/// `VRC3` (Mapper 073).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc3 {
    /// The hard-wired nametable arrangement.
    pub mirroring: Mirroring,
    /// `$F000`: the 16K PRG bank at `$8000`.
    pub prg_bank: u8,
    /// `$8000-$B000`: the value a `$C000` write or a wrap loads into the counter, a nibble per
    /// register.
    pub irq_reload: u16,
    /// The CPU-cycle up-counter.
    pub irq_counter: u16,
    /// `$C000` bit 1: whether the counter runs.
    pub irq_enabled: bool,
    /// `$C000` bit 0: the value `$D000` copies into `irq_enabled`.
    pub irq_enable_on_ack: bool,
    /// `$C000` bit 2: whether only the low 8 bits of the counter count and reload.
    pub irq_8bit: bool,
    /// The counter wrapped and the IRQ is not yet acknowledged.
    pub irq_pending: bool,
}

impl Vrc3 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K CHR-RAM
    // CPU $6000..=$7FFF 8K PRG-RAM
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg_bank: 0,
            irq_reload: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_enable_on_ack: false,
            irq_8bit: false,
            irq_pending: false,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Vrc3 {
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
        out.push(("PRG $8000", u32::from(self.prg_bank)));
        out.push(("IRQ counter", u32::from(self.irq_counter)));
        out.push(("IRQ reload", u32::from(self.irq_reload)));
        out.push(("IRQ enabled", u32::from(self.irq_enabled)));
        out.push(("IRQ 8-bit", u32::from(self.irq_8bit)));
        out.push(("IRQ pending", u32::from(self.irq_pending)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr & 0xF000 {
            reg @ 0x8000..=0xB000 => {
                let shift = (reg - 0x8000) >> 10;
                let nibble = u16::from(val & 0x0F) << shift;
                self.irq_reload = (self.irq_reload & !(0x0F << shift)) | nibble;
            }
            0xC000 => {
                self.irq_enable_on_ack = val & 0x01 == 0x01;
                self.irq_enabled = val & 0x02 == 0x02;
                self.irq_8bit = val & 0x04 == 0x04;
                if self.irq_enabled {
                    self.irq_counter = self.irq_reload;
                }
                self.irq_pending = false;
            }
            0xD000 => {
                self.irq_enabled = self.irq_enable_on_ack;
                self.irq_pending = false;
            }
            0xF000 => {
                self.prg_bank = val & 0x0F;
                memory.map_prg(
                    0x8000,
                    Self::PRG_WINDOW,
                    i32::from(self.prg_bank),
                    Src::PrgRom,
                );
            }
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, 8 * 1024, 0, Src::PrgRam);
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        if !self.irq_enabled {
            return;
        }
        if self.irq_8bit {
            // The low byte counts and reloads alone, and the high byte never changes.
            let low = (self.irq_counter as u8).wrapping_add(1);
            let low = if low == 0 {
                self.irq_pending = true;
                self.irq_reload as u8
            } else {
                low
            };
            self.irq_counter = (self.irq_counter & 0xFF00) | u16::from(low);
        } else {
            self.irq_counter = self.irq_counter.wrapping_add(1);
            if self.irq_counter == 0 {
                self.irq_pending = true;
                self.irq_counter = self.irq_reload;
            }
        }
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.irq_enabled = false;
        self.irq_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks), 8K PRG-RAM and 8K CHR-RAM.
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 0);
        let mapper = Vrc3::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Writes the 16-bit reload value as four nibbles, low nibble at `$8000`.
    fn reload(mapper: &mut Mapper, cart: &mut Cart, val: u16) {
        for (nibble, reg) in [0x8000, 0x9000, 0xA000, 0xB000].into_iter().enumerate() {
            write(mapper, cart, reg, (val >> (nibble * 4)) as u8 & 0x0F);
        }
    }

    /// `$F000` banks `$8000` under a fixed last bank.
    #[test]
    fn prg_register() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xF000, 5);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 5 * 16, "PRG 5");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "fixed last");
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "PRG-RAM");
    }

    /// In 16-bit mode the counter counts up from the reload value, fires on the `$FFFF` wrap,
    /// and reloads.
    #[test]
    fn irq_16bit_fires_on_wrap_and_reloads() {
        let (mut mapper, mut cart) = load();
        reload(&mut mapper, &mut cart, 0xFFFD);
        write(&mut mapper, &mut cart, 0xC000, 0x02);
        for clock in 0..2 {
            mapper.clock();
            assert!(!mapper.irq_pending(), "too early, at clock {clock}");
        }
        mapper.clock();
        assert!(mapper.irq_pending(), "fires on the wrap");

        write(&mut mapper, &mut cart, 0xD000, 0);
        assert!(!mapper.irq_pending(), "$D000 acknowledges");
        for _ in 0..3 {
            mapper.clock();
        }
        assert!(
            !mapper.irq_pending(),
            "$D000 copied a clear enable-on-ack bit"
        );
    }

    /// In 8-bit mode only the low byte counts, it reloads only the low byte, and the high byte
    /// does not move.
    #[test]
    fn irq_8bit_counts_the_low_byte_only() {
        let (mut mapper, mut cart) = load();
        reload(&mut mapper, &mut cart, 0x12FE);
        write(&mut mapper, &mut cart, 0xC000, 0x07);
        mapper.clock();
        assert!(!mapper.irq_pending());
        mapper.clock();
        assert!(mapper.irq_pending(), "fires on the low byte wrap");
        let Mapper::Vrc3(vrc3) = &mapper else {
            panic!("expected a VRC3");
        };
        assert_eq!(
            vrc3.irq_counter, 0x12FE,
            "low byte reloaded, high byte untouched"
        );

        write(&mut mapper, &mut cart, 0xD000, 0);
        assert!(!mapper.irq_pending(), "$D000 acknowledges");
        mapper.clock();
        mapper.clock();
        assert!(
            mapper.irq_pending(),
            "enable-on-ack kept the counter running"
        );
    }
}
