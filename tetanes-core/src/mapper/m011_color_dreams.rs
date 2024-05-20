//! `Color Dreams` (Mapper 011)
//!
//! <http://wiki.nesdev.com/w/index.php/Color_Dreams>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset},
    mapper::{Mapped, MappedRead, MappedWrite, Mapper, MemMap, Mirroring},
    mem::MemBanks,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct ColorDreams {
    pub mirroring: Mirroring,
    pub chr_banks: MemBanks,
    pub prg_rom_banks: MemBanks,
}

impl ColorDreams {
    const PRG_WINDOW: usize = 32 * 1024;
    const CHR_ROM_WINDOW: usize = 8 * 1024;

    const CHR_BANK_MASK: u8 = 0xF0; // 0b1111 0000
    const PRG_BANK_MASK: u8 = 0x03; // 0b0000 0011

    pub fn load(cart: &mut Cart) -> Mapper {
        let color_dreams = Self {
            mirroring: cart.mirroring(),
            chr_banks: MemBanks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_ROM_WINDOW),
            prg_rom_banks: MemBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_WINDOW),
        };
        color_dreams.into()
    }

    pub fn update_banks(&mut self) {
        todo!()
    }
}

impl Mapped for ColorDreams {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MemMap for ColorDreams {
    // PPU $0000..=$1FFF 8K switchable CHR-ROM bank
    // CPU $8000..=$FFFF 32K switchable PRG-ROM bank

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Bus,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        if matches!(addr, 0x8000..=0xFFFF) {
            self.chr_banks
                .set(0, ((val & Self::CHR_BANK_MASK) >> 4).into());
            self.prg_rom_banks
                .set(0, (val & Self::PRG_BANK_MASK).into());
        }
        MappedWrite::Bus
    }
}

impl Clock for ColorDreams {}
impl Regional for ColorDreams {}
impl Reset for ColorDreams {}
