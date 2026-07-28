//! `BF909x` (Mapper 071).
//!
//! <https://wiki.nesdev.org/w/index.php/INES_Mapper_071>

use crate::{
    cart::Cart,
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `BF909x` board revision.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    #[default]
    Bf909x,
    Bf9097,
}

/// `BF909x` (Mapper 071).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bf909x {
    pub revision: Revision,
    pub mirroring: Mirroring,
    pub prg_bank: u8,
}

impl Bf909x {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    const SINGLE_SCREEN_A: u8 = 0x10;

    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable
    // CPU $C000..=$FFFF 16K PRG-ROM Fixed to Last Bank
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            revision: if cart.submapper_num() == 1 {
                Revision::Bf9097
            } else {
                Revision::Bf909x
            },
            mirroring: cart.mirroring(),
            prg_bank: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    pub const fn set_revision(&mut self, rev: Revision) {
        self.revision = rev;
    }
}

impl Map for Bf909x {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        // Firehawk selects mirroring at $9000; any board that writes there is a BF9097.
        if addr == 0x9000 {
            self.revision = Revision::Bf9097;
        }
        if addr >= 0xC000 || self.revision != Revision::Bf9097 {
            self.prg_bank = val;
            memory.map_prg(0x8000, Self::PRG_WINDOW, i32::from(val), Src::PrgRom);
        } else {
            self.mirroring = if val & Self::SINGLE_SCREEN_A == Self::SINGLE_SCREEN_A {
                Mirroring::SingleScreenA
            } else {
                Mirroring::SingleScreenB
            };
            memory.set_mirroring(self.mirroring);
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks), no PRG-RAM, 8K CHR-RAM.
    fn load(submapper: u8) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 0);
        cart.header.submapper_num = submapper;
        let mapper = Bf909x::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    fn bf909x() -> (Mapper, Cart) {
        load(0)
    }

    /// Which board the loader (or the $9000 heuristic) settled on.
    fn revision(mapper: &Mapper) -> Revision {
        match mapper {
            Mapper::Bf909x(board) => board.revision,
            board => unreachable!("expected a Bf909x, got {board:?}"),
        }
    }

    /// $C000 is fixed to the last bank from power-on, which is what puts a reset vector under the
    /// CPU before the game has written its bank register.
    #[test]
    fn powers_on_with_bank_0_low_and_the_last_bank_fixed_high() {
        let (mapper, cart) = bf909x();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "last bank");
    }

    /// On a plain BF909x every write above $8000 is the bank register - the Codemasters games
    /// write it at $C000, but the board decodes no address bits below that.
    #[test]
    fn a_bf909x_takes_a_bank_from_any_address_above_8000() {
        let (mut mapper, mut cart) = bf909x();
        for (addr, bank) in [(0x8000, 3), (0xA000, 4), (0xC000, 5), (0xFFFF, 6)] {
            write(&mut mapper, &mut cart, addr, bank);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), bank * 16, "${addr:04X}");
            assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "still fixed");
        }
    }

    /// Fire Hawk is the one cart with mapper-controlled mirroring, and it is flagged by NES 2.0
    /// submapper 1.
    #[test]
    fn submapper_1_is_a_bf9097() {
        let (mapper, _cart) = load(1);
        assert_eq!(revision(&mapper), Revision::Bf9097);
        assert_eq!(revision(&bf909x().0), Revision::Bf909x, "and 0 is not");
    }

    /// A BF9097 splits the range: $C000 and up is the bank register, below it is mirroring.
    ///
    /// NB the polarity follows Mesen, which this board was ported from - bit 4 set selects
    /// nametable A. FCEUX has it the other way round (`MI_0 + ((V >> 4) & 1)`), and the wiki says
    /// only that the bit "selects the 1 KiB CIRAM bank" without pinning which. Left as Mesen has
    /// it; there is no Fire Hawk ROM here to settle it.
    #[test]
    fn a_bf9097_splits_the_range_into_mirroring_and_banking() {
        let (mut mapper, mut cart) = load(1);

        write(&mut mapper, &mut cart, 0xC000, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "$C000 banks");

        write(&mut mapper, &mut cart, 0x9000, 0x10);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenA);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "$9000 does not");
        cart.memory.chr_write(0x2000, 0xAA);
        for nt in [0x2400, 0x2800, 0x2C00] {
            assert_eq!(chr_peek(&mapper, &cart, nt), 0xAA, "one-screen A");
        }

        write(&mut mapper, &mut cart, 0x9000, 0x00);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB);
        cart.memory.chr_write(0x2000, 0xBB);
        assert_eq!(chr_peek(&mapper, &cart, 0x2C00), 0xBB, "one-screen B");
        // The two screens are separate CIRAM banks, so switching back finds the old contents.
        write(&mut mapper, &mut cart, 0x9000, 0x10);
        assert_eq!(chr_peek(&mapper, &cart, 0x2000), 0xAA);
    }

    /// Only Fire Hawk writes $9000, so a write there identifies the board even when the header
    /// did not - and that same write must take effect as mirroring rather than as a bank.
    #[test]
    fn a_write_to_9000_auto_detects_a_bf9097() {
        let (mut mapper, mut cart) = bf909x();
        write(&mut mapper, &mut cart, 0x8000, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "still a BF909x");

        write(&mut mapper, &mut cart, 0x9000, 0x10);
        assert_eq!(revision(&mapper), Revision::Bf9097, "detected");
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenA);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "not a bank write");

        // And from here on $8000-$BFFF is mirroring, not banking.
        write(&mut mapper, &mut cart, 0xA000, 0x00);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16);
    }

    /// The board decodes only $8000-$FFFF; below that is the bus, not a register.
    #[test]
    fn writes_below_8000_are_ignored() {
        let (mut mapper, mut cart) = bf909x();
        write(&mut mapper, &mut cart, 0x8000, 3);
        for addr in [0x4100, 0x6000, 0x7FFF] {
            write(&mut mapper, &mut cart, addr, 5);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "${addr:04X}");
        }
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = load(1);
        write(&mut mapper, &mut cart, 0xC000, 3);
        write(&mut mapper, &mut cart, 0x9000, 0x10);

        let sample = |mapper: &Mapper, cart: &Cart| {
            [
                prg_peek(mapper, cart, 0x8000),
                prg_peek(mapper, cart, 0xC000),
                chr_peek(mapper, cart, 0x0000),
            ]
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenA, "mirroring too");
    }
}
