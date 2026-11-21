//! `Sunsoft-3` (Mapper 067).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_067>

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

/// `Sunsoft-3` (Mapper 067).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sunsoft3 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// `$F800`: the 16K PRG bank at `$8000`.
    pub prg_bank: u8,
    /// `$8800`, `$9800`, `$A800` and `$B800`: 2K CHR banks.
    pub chr: [u8; 4],
    /// `$D800` bit 4: whether the counter runs.
    pub irq_enabled: bool,
    /// The counter wrapped and the IRQ is not yet acknowledged.
    pub irq_pending: bool,
    /// Whether the next `$C800` write sets the low byte of the counter instead of the high byte.
    pub irq_write_low: bool,
    /// The CPU-cycle down-counter.
    pub irq_counter: u16,
}

impl Sunsoft3 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 2 * 1024;

    // PPU $0000..=$1FFF four 2K switchable CHR-ROM banks
    // CPU $6000..=$7FFF 8K PRG-RAM
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg_bank: 0,
            chr: [0; 4],
            irq_enabled: false,
            irq_pending: false,
            irq_write_low: false,
            irq_counter: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Sunsoft3 {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("PRG $8000", u32::from(self.prg_bank)));
        for (name, bank) in ["CHR $0000", "CHR $0800", "CHR $1000", "CHR $1800"]
            .into_iter()
            .zip(self.chr)
        {
            out.push((name, u32::from(bank)));
        }
        out.push(("IRQ counter", u32::from(self.irq_counter)));
        out.push(("IRQ enabled", u32::from(self.irq_enabled)));
        out.push(("IRQ pending", u32::from(self.irq_pending)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr & 0xF800 {
            reg @ (0x8800 | 0x9800 | 0xA800 | 0xB800) => {
                let slot = usize::from((reg - 0x8800) >> 12);
                self.chr[slot] = val;
                let addr = (slot * Self::CHR_WINDOW) as u16;
                memory.map_chr(addr, Self::CHR_WINDOW, i32::from(val), Src::Chr);
            }
            // Written twice, high byte first, straight into the counter.
            0xC800 => {
                self.irq_counter = if self.irq_write_low {
                    (self.irq_counter & 0xFF00) | u16::from(val)
                } else {
                    (self.irq_counter & 0x00FF) | (u16::from(val) << 8)
                };
                self.irq_write_low = !self.irq_write_low;
            }
            0xD800 => {
                self.irq_enabled = val & 0x10 == 0x10;
                self.irq_write_low = false;
                self.irq_pending = false;
            }
            0xE800 => {
                self.mirroring = match val & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenA,
                    _ => Mirroring::SingleScreenB,
                };
                memory.set_mirroring(self.mirroring);
            }
            0xF800 => {
                self.prg_bank = val;
                memory.map_prg(0x8000, Self::PRG_WINDOW, i32::from(val), Src::PrgRom);
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
        for (slot, bank) in self.chr.iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*bank), Src::Chr);
        }
        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        // The counter fires on its $0000 to $FFFF wrap and then stops itself.
        if self.irq_enabled {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
            if self.irq_counter == 0xFFFF {
                self.irq_enabled = false;
                self.irq_pending = true;
            }
        }
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.irq_enabled = false;
        self.irq_pending = false;
        self.irq_write_low = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks), 8K PRG-RAM and 128K CHR-ROM (64 2K banks).
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 128 * 1024);
        let mapper = Sunsoft3::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// `$F800` banks `$8000` under a fixed last bank, and the four CHR registers decode on
    /// `$x800` with the low bits ignored.
    #[test]
    fn prg_and_chr_registers() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xF800, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "PRG 3");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "fixed last");
        for (reg, bank) in [(0x8800, 10), (0x9FFF, 11), (0xA800, 12), (0xB801, 13)] {
            write(&mut mapper, &mut cart, reg, bank);
        }
        for (slot, bank) in [10u8, 11, 12, 13].into_iter().enumerate() {
            assert_eq!(
                chr_peek(&mapper, &cart, slot as u16 * 0x800),
                0x80 | (bank * 2),
                "CHR slot {slot}"
            );
        }
        // $8000-$87FF decodes to no register.
        write(&mut mapper, &mut cart, 0x8000, 0);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 20, "unchanged");
    }

    /// `$E800` picks one of four nametable arrangements.
    #[test]
    fn mirroring_register() {
        let (mut mapper, mut cart) = load();
        for (val, mirroring) in [
            (0, Mirroring::Vertical),
            (1, Mirroring::Horizontal),
            (2, Mirroring::SingleScreenA),
            (3, Mirroring::SingleScreenB),
        ] {
            write(&mut mapper, &mut cart, 0xE800, val);
            assert_eq!(mapper.mirroring(), mirroring, "${val:02X}");
        }
    }

    /// `$C800` loads the counter high byte first. The IRQ fires on the wrap past zero, one cycle
    /// after the counter reads zero, and then the counter stops.
    #[test]
    fn irq_fires_on_wrap_and_disables_itself() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xC800, 0x01);
        write(&mut mapper, &mut cart, 0xC800, 0x02);
        write(&mut mapper, &mut cart, 0xD800, 0x10);
        for clock in 0..0x0102 {
            mapper.clock();
            assert!(!mapper.irq_pending(), "too early, at clock {clock}");
        }
        mapper.clock();
        assert!(mapper.irq_pending(), "fires on the $0000 to $FFFF wrap");

        write(&mut mapper, &mut cart, 0xD800, 0x00);
        assert!(!mapper.irq_pending(), "$D800 acknowledges");
        for _ in 0..0x20000 {
            mapper.clock();
        }
        assert!(!mapper.irq_pending(), "the counter disabled itself");
    }

    /// `$D800` resets the `$C800` write toggle, so the next write is the high byte again.
    #[test]
    fn d800_resets_the_write_toggle() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xC800, 0xAA);
        write(&mut mapper, &mut cart, 0xD800, 0x00);
        write(&mut mapper, &mut cart, 0xC800, 0x00);
        write(&mut mapper, &mut cart, 0xC800, 0x01);
        write(&mut mapper, &mut cart, 0xD800, 0x10);
        mapper.clock();
        assert!(!mapper.irq_pending(), "counter is $0001, not $AA01 or $0100");
        mapper.clock();
        assert!(mapper.irq_pending());
    }
}
