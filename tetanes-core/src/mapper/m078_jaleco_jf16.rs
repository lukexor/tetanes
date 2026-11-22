//! `Jaleco JF-16` and `Irem IF-12` (Mapper 078).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_078>

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

/// `Jaleco JF-16` and `Irem IF-12` (Mapper 078).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct JalecoJf16 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// Submapper 3 (Irem's IF-12, used by Holy Diver) switches between horizontal and vertical
    /// mirroring. Any other submapper is Jaleco's JF-16, which switches between one-screen
    /// nametables.
    pub submapper_num: u8,
    /// The selected 16K PRG bank at `$8000`.
    pub prg_bank: u8,
    /// The selected 8K CHR bank.
    pub chr_bank: u8,
}

impl JalecoJf16 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K switchable CHR-ROM bank
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let submapper_num = cart.submapper_num();
        let mut board = Self {
            // Both games' headers set the four-screen bit, which the board does not have, so the
            // mirroring starts as a cleared register bit decodes.
            mirroring: Self::decode_mirroring(submapper_num, false),
            submapper_num,
            prg_bank: 0,
            chr_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    /// The nametable arrangement register bit 3 selects on this board.
    const fn decode_mirroring(submapper_num: u8, bit: bool) -> Mirroring {
        match (submapper_num == 3, bit) {
            (true, true) => Mirroring::Vertical,
            (true, false) => Mirroring::Horizontal,
            (false, true) => Mirroring::SingleScreenB,
            (false, false) => Mirroring::SingleScreenA,
        }
    }
}

impl Map for JalecoJf16 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("PRG $8000", u32::from(self.prg_bank)));
        out.push(("CHR $0000", u32::from(self.chr_bank)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        // The ROM drives the data bus too, so the register sees the AND of both.
        let val = val & memory.prg_peek(addr);
        // [CCCC MPPP]
        self.prg_bank = val & 0x07;
        self.chr_bank = val >> 4;
        self.mirroring = Self::decode_mirroring(self.submapper_num, val & 0x08 == 0x08);
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks) and 128K CHR-ROM (16 8K banks). The last PRG byte is `$FF`, so
    /// a write to `$FFFF` passes through bus conflicts unchanged.
    fn load(submapper_num: u8) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 128 * 1024);
        cart.header.submapper_num = submapper_num;
        cart.memory.region_mut(Src::PrgRom)[0x1FFFF] = 0xFF;
        let mapper = JalecoJf16::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// The register banks PRG from bits 0-2 and CHR from bits 4-7, and `$C000` stays on the last
    /// bank.
    #[test]
    fn register_banks_prg_and_chr() {
        let (mut mapper, mut cart) = load(0);
        write(&mut mapper, &mut cart, 0xFFFF, 0xA5);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 5 * 16, "PRG 5");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "fixed last");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (10 * 8), "CHR 10");
    }

    /// Bit 3 picks between one-screen nametables, except on submapper 3 (Holy Diver), where it
    /// picks between horizontal and vertical.
    #[test]
    fn mirroring_depends_on_submapper() {
        for (submapper, clear, set) in [
            (0, Mirroring::SingleScreenA, Mirroring::SingleScreenB),
            (3, Mirroring::Horizontal, Mirroring::Vertical),
        ] {
            let (mut mapper, mut cart) = load(submapper);
            write(&mut mapper, &mut cart, 0xFFFF, 0x00);
            assert_eq!(
                mapper.mirroring(),
                clear,
                "submapper {submapper}, bit clear"
            );
            write(&mut mapper, &mut cart, 0xFFFF, 0x08);
            assert_eq!(mapper.mirroring(), set, "submapper {submapper}, bit set");
        }
    }
}
