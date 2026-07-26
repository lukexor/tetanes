//! `CNROM (Mapper 003)`.
//!
//! <https://wiki.nesdev.org/w/index.php/CNROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `CNROM` (Mapper 003).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Cnrom {
    pub mirroring: Mirroring,
    pub chr_bank: u8,
}

impl Cnrom {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    // PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
    // CPU $8000..=$FFFF 16K or 32K Fixed PRG-ROM
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        // A 16K cart maps the same bank into both slots, which falls out of the bank index
        // wrapping within the region.
        let mut board = Self {
            mirroring: cart.mirroring(),
            chr_bank: 0,
        };
        board.sync(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Cnrom {

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 {
            self.chr_bank = val;
            memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(val), Src::Chr);
        }
    }

    fn sync(&mut self, memory: &mut Memory) {
        // A 16K cart maps the same bank into both slots, which falls out of the bank index
        // wrapping within the region.
        memory.map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(self.chr_bank), Src::Chr);
        memory.set_mirroring(self.mirroring);
    }
}

impl Reset for Cnrom {}
impl Clock for Cnrom {}
impl Regional for Cnrom {}
impl Sram for Cnrom {}
