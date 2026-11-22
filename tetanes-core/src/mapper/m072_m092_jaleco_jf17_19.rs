//! `Jaleco JF-17` (Mapper 072) and `Jaleco JF-19` (Mapper 092).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_072>
//! <https://www.nesdev.org/wiki/INES_Mapper_092>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `Jaleco JF-17` (Mapper 072) and `Jaleco JF-19` (Mapper 092).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct JalecoJf17 {
    /// The hard-wired nametable arrangement.
    pub mirroring: Mirroring,
    /// JF-19 (mapper 092) banks `$C000` and fixes `$8000` to the first bank. JF-17 (mapper 072)
    /// banks `$8000` and fixes `$C000` to the last.
    pub jf19: bool,
    /// The selected 16K PRG bank.
    pub prg_bank: u8,
    /// The selected 8K CHR bank.
    pub chr_bank: u8,
    /// The last write's bit 7, whose rising edge latches the PRG bank.
    pub prg_latch: bool,
    /// The last write's bit 6, whose rising edge latches the CHR bank.
    pub chr_latch: bool,
}

impl JalecoJf17 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K switchable CHR-ROM bank
    // CPU $8000..=$BFFF 16K PRG-ROM, switchable on JF-17 and fixed to the first bank on JF-19
    // CPU $C000..=$FFFF 16K PRG-ROM, fixed to the last bank on JF-17 and switchable on JF-19
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart, jf19: bool) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            jf19,
            // JF-19 must start with the last bank at $C000, where the CPU reads its reset vector.
            // The index wraps to the last bank of any PRG size.
            prg_bank: if jf19 { 0x0F } else { 0 },
            chr_bank: 0,
            prg_latch: false,
            chr_latch: false,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for JalecoJf17 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        let prg = if self.jf19 { "PRG $C000" } else { "PRG $8000" };
        out.push((prg, u32::from(self.prg_bank)));
        out.push(("CHR $0000", u32::from(self.chr_bank)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        // The ROM drives the data bus too, so the register sees the AND of both.
        let val = val & memory.prg_peek(addr);
        // [PC.. BBBB]: P and C latch the bank bits into PRG or CHR on their rising edge. Bits 4-5
        // drive the speech chip, whose samples are not in the dump.
        let prg_latch = val & 0x80 == 0x80;
        let chr_latch = val & 0x40 == 0x40;
        if prg_latch && !self.prg_latch {
            self.prg_bank = val & 0x0F;
        }
        if chr_latch && !self.chr_latch {
            self.chr_bank = val & 0x0F;
        }
        self.prg_latch = prg_latch;
        self.chr_latch = chr_latch;
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        let bank = i32::from(self.prg_bank);
        let (bank_8000, bank_c000) = if self.jf19 { (0, bank) } else { (bank, -1) };
        memory.map_prg(0x8000, Self::PRG_WINDOW, bank_8000, Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, bank_c000, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks) and 64K CHR-ROM (8 8K banks).
    fn load(jf19: bool) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 64 * 1024);
        let mapper = JalecoJf17::load(&mut cart, jf19).expect("valid mapper");
        (mapper, cart)
    }

    /// Bank bits only take effect on a 0-to-1 edge of the latch bit, and a held latch ignores
    /// new bank bits.
    #[test]
    fn banks_latch_on_the_rising_edge() {
        let (mut mapper, mut cart) = load(false);
        // $FC00 sits in the last 1K page, which reads $7F, so bus conflicts keep bits 0-6.
        let reg = 0xFC00;
        write(&mut mapper, &mut cart, reg, 0x43);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (3 * 8), "CHR 3");
        write(&mut mapper, &mut cart, reg, 0x45);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0000),
            0x80 | (3 * 8),
            "held latch"
        );
        write(&mut mapper, &mut cart, reg, 0x05);
        write(&mut mapper, &mut cart, reg, 0x45);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (5 * 8), "new edge");
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "PRG untouched");
    }

    /// JF-17 banks `$8000` and fixes `$C000` to the last bank.
    #[test]
    fn jf17_banks_8000() {
        let (mut mapper, mut cart) = load(false);
        // No page byte has bit 7 set, so the ROM byte under the write is set by hand to let the
        // PRG latch bit through bus conflicts. $FFFF is the last byte of the fixed last bank.
        cart.memory.region_mut(Src::PrgRom)[0x1FFFF] = 0xFF;
        write(&mut mapper, &mut cart, 0xFFFF, 0x83);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "$8000 bank 3");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "$C000 fixed last");
    }

    /// JF-19 fixes `$8000` to the first bank and banks `$C000`.
    #[test]
    fn jf19_banks_c000() {
        let (mut mapper, mut cart) = load(true);
        assert_eq!(
            prg_peek(&mapper, &cart, 0xC000),
            7 * 16,
            "powers on as the last bank"
        );
        cart.memory.region_mut(Src::PrgRom)[0x1FFFF] = 0xFF;
        write(&mut mapper, &mut cart, 0xFFFF, 0x83);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "$8000 fixed first");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 3 * 16, "$C000 bank 3");
    }

    /// The ROM byte under the write ANDs into the value.
    #[test]
    fn writes_have_bus_conflicts() {
        let (mut mapper, mut cart) = load(false);
        // $8C00 is PRG page 3, so it reads $03 and the latch bit never rises.
        write(&mut mapper, &mut cart, 0x8C00, 0xC7);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80, "no latch edge");
    }
}
