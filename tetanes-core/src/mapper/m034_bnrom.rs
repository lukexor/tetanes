//! `BNROM (Mapper 034)`.
//!
//! <https://wiki.nesdev.org/w/index.php/BNROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `BNROM` (Mapper 034).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bnrom {
    pub mirroring: Mirroring,
    pub prg_bank: u8,
}

impl Bnrom {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg_bank: 0,
        };
        board.sync(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Bnrom {
    fn uses_page_tables(&self) -> bool {
        true
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            self.prg_bank = val;
            memory.map_prg(0x8000, Self::PRG_WINDOW, i32::from(val), Src::PrgRom);
        }
    }

    fn sync(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

impl Reset for Bnrom {}
impl Clock for Bnrom {}
impl Regional for Bnrom {}
impl Sram for Bnrom {}
