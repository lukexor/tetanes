//! `UxROM (Mapper 002)`.
//!
//! <https://wiki.nesdev.org/w/index.php/UxROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `UxROM` (Mapper 002).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Uxrom {
    pub mirroring: Mirroring,
    pub prg_bank: u8,
}

impl Uxrom {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable
    // CPU $C000..=$FFFF 16K PRG-ROM Fixed to Last Bank
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        cart.memory
            .map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
        cart.memory
            .map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        cart.memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        cart.memory.set_mirroring(cart.mirroring());
        Ok(Self {
            mirroring: cart.mirroring(),
            prg_bank: 0,
        }
        .into())
    }
}

impl Map for Uxrom {
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
}

impl Reset for Uxrom {}
impl Clock for Uxrom {}
impl Regional for Uxrom {}
impl Sram for Uxrom {}
