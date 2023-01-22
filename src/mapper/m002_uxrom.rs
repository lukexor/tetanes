//! `UxROM` (Mapper 002)
//!
//! <https://wiki.nesdev.com/w/index.php/UxROM>

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
pub struct Uxrom {
    mirroring: Mirroring,
    prg_rom_banks: MemBanks,
}

impl Uxrom {
    const PRG_ROM_WINDOW: usize = 16 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    pub fn load(cart: &mut Cart) -> Mapper {
        if !cart.has_chr() {
            cart.add_chr_ram(Self::CHR_RAM_SIZE);
        };
        let mut uxrom = Self {
            mirroring: cart.mirroring(),
            prg_rom_banks: MemBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW),
        };
        let last_bank = uxrom.prg_rom_banks.last();
        uxrom.prg_rom_banks.set(1, last_bank);
        uxrom.into()
    }
}

impl MemMap for Uxrom {
    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable
    // CPU $C000..=$FFFF 16K PRG-ROM Fixed to Last Bank

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr.into()),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::None,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(addr.into(), val),
            0x8000..=0xFFFF => {
                self.prg_rom_banks.set(0, val.into());
                MappedWrite::None
            }
            _ => MappedWrite::None,
        }
    }
}

impl Mapped for Uxrom {
    #[inline]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    #[inline]
    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl Clock for Uxrom {}
impl Regional for Uxrom {}
impl Reset for Uxrom {}
