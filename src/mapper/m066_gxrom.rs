//! `GxROM` (Mapper 066)
//!
//! <https://wiki.nesdev.org/w/index.php?title=GxROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset},
    mapper::{Mapped, MappedRead, MappedWrite, Mapper, MemMap},
    mem::MemBanks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Gxrom {
    mirroring: Mirroring,
    // PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
    chr_banks: MemBanks,
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable
    prg_rom_banks: MemBanks,
}

impl Gxrom {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    const CHR_BANK_MASK: u8 = 0x0F; // 0b1111
    const PRG_BANK_MASK: u8 = 0x30; // 0b110000

    pub fn load(cart: &mut Cart) -> Mapper {
        let gxrom = Self {
            mirroring: cart.mirroring(),
            chr_banks: MemBanks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_WINDOW),
            prg_rom_banks: MemBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW),
        };
        gxrom.into()
    }
}

impl MemMap for Gxrom {
    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Default,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        if matches!(addr, 0x8000..=0xFFFF) {
            self.chr_banks.set(0, (val & Self::CHR_BANK_MASK).into());
            self.prg_rom_banks
                .set(0, ((val & Self::PRG_BANK_MASK) >> 4).into());
        }
        MappedWrite::Default
    }
}

impl Mapped for Gxrom {
    #[inline]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    #[inline]
    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl Clock for Gxrom {}
impl Regional for Gxrom {}
impl Reset for Gxrom {}
