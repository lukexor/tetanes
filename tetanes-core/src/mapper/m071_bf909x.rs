//! `BF909x` (Mapper 071).
//!
//! <https://wiki.nesdev.org/w/index.php/INES_Mapper_071>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
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
        board.sync(&mut cart.memory);
        Ok(board.into())
    }

    pub const fn set_revision(&mut self, rev: Revision) {
        self.revision = rev;
    }
}

impl Map for Bf909x {
    fn uses_page_tables(&self) -> bool {
        true
    }

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

    fn sync(&mut self, memory: &mut Memory) {
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

impl Reset for Bf909x {}
impl Clock for Bf909x {}
impl Regional for Bf909x {}
impl Sram for Bf909x {}
