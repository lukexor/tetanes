//! `Irem LROG017` (Mapper 077).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_077>

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

/// `Irem LROG017` (Mapper 077).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct IremLrog017 {
    /// The selected 32K PRG bank.
    pub prg_bank: u8,
    /// The selected 2K CHR-ROM bank at `$0000`.
    pub chr_bank: u8,
}

impl IremLrog017 {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 2 * 1024;
    const NAMETABLE_WINDOW: usize = 1024;

    // PPU $0000..=$07FF 2K switchable CHR-ROM bank
    // PPU $0800..=$1FFF 6K CHR-RAM, held in expansion RAM
    // PPU $2000..=$27FF two nametables in CIRAM
    // PPU $2800..=$2FFF two nametables, held in expansion RAM
    // CPU $8000..=$FFFF 32K switchable PRG-ROM bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            prg_bank: 0,
            chr_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for IremLrog017 {
    fn mirroring(&self) -> Mirroring {
        Mirroring::FourScreen
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
        // [CCCC PPPP]
        self.prg_bank = val & 0x0F;
        self.chr_bank = val >> 4;
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
        // The 6K of CHR-RAM is the second through fourth 2K of expansion RAM, so each PPU address
        // reads the same offset of it.
        for bank in 1..4 {
            let addr = (bank * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, bank as i32, Src::ExRam);
        }
        // The first 2K of the board's RAM is the second pair of nametables, at $2800-$2FFF, so
        // four-screen works whatever CIRAM size the header asked for.
        let nametables = [
            (Src::CiRam, 0),
            (Src::CiRam, 1),
            (Src::ExRam, 0),
            (Src::ExRam, 1),
        ];
        for (nametable, (src, bank)) in nametables.into_iter().enumerate() {
            let addr = 0x2000 + (nametable * Self::NAMETABLE_WINDOW) as u16;
            memory.map_chr(addr, Self::NAMETABLE_WINDOW, bank, src);
            // $3000-$3EFF mirrors $2000-$2EFF.
            memory.map_chr(addr + 0x1000, Self::NAMETABLE_WINDOW, bank, src);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// The register banks 32K of PRG from the low nibble and 2K of CHR-ROM from the high one,
    /// through bus conflicts.
    #[test]
    fn register_banks_prg_and_chr_rom() {
        let mut cart = page_indexed_cart(128 * 1024, 0, 32 * 1024);
        let mut mapper = IremLrog017::load(&mut cart).expect("valid mapper");
        // Every PRG page byte is below $80, so set the byte under the write to let all bits by.
        cart.memory.region_mut(Src::PrgRom)[0x7FFF] = 0xFF;
        write(&mut mapper, &mut cart, 0xFFFF, 0x53);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 32, "PRG 3");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (5 * 2), "CHR 5");

        // $8000 is now PRG page 96, which reads $60, and the AND keeps only bits 5-6.
        write(&mut mapper, &mut cart, 0x8000, 0xFF);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "PRG 0");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (6 * 2), "CHR 6");
    }

    /// `$0800-$1FFF` is CHR-RAM in expansion RAM, at the same offset as the PPU address.
    #[test]
    fn upper_6k_is_chr_ram() {
        let mut cart = page_indexed_cart(128 * 1024, 0, 32 * 1024);
        let mapper = IremLrog017::load(&mut cart).expect("valid mapper");
        let ex_ram = cart.memory.region_mut(Src::ExRam);
        ex_ram[0x0800] = 0x11;
        ex_ram[0x1FFF] = 0x22;
        assert_eq!(chr_peek(&mapper, &cart, 0x0800), 0x11);
        assert_eq!(chr_peek(&mapper, &cart, 0x1FFF), 0x22);
    }

    /// All four nametables are distinct even with only 2K of CIRAM, because `$2800-$2FFF` comes
    /// from the first 2K of expansion RAM.
    #[test]
    fn four_nametables_without_a_four_screen_header() {
        let mut cart = page_indexed_cart(128 * 1024, 0, 32 * 1024);
        let mapper = IremLrog017::load(&mut cart).expect("valid mapper");
        for (page, mark) in [0x31, 0x32].into_iter().enumerate() {
            cart.memory.region_mut(Src::CiRam)[page * 1024] = mark;
        }
        for (page, mark) in [0x33, 0x34].into_iter().enumerate() {
            cart.memory.region_mut(Src::ExRam)[page * 1024] = mark;
        }
        for (addr, mark) in [(0x2000, 0x31), (0x2400, 0x32), (0x2800, 0x33), (0x2C00, 0x34)] {
            assert_eq!(chr_peek(&mapper, &cart, addr), mark, "${addr:04X}");
            assert_eq!(chr_peek(&mapper, &cart, addr + 0x1000), mark, "${addr:04X} mirror");
        }
    }
}
