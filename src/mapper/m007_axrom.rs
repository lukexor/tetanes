//! `AxROM` (Mapper 007)
//!
//! <https://wiki.nesdev.com/w/index.php/AxROM>

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
pub struct Axrom {
    mirroring: Mirroring,
    // PPU $0000..=$1FFF 8K CHR-RAM Bank Fixed
    // CPU $8000..=$FFFF 32K switchable PRG-ROM bank
    prg_rom_banks: MemBanks,
}

impl Axrom {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;
    const SINGLE_SCREEN_B: u8 = 0x10; // 0b10000

    pub fn load(cart: &mut Cart) -> Mapper {
        if !cart.has_chr() {
            cart.add_chr_ram(Self::CHR_RAM_SIZE);
        }
        let axrom = Self {
            mirroring: cart.mirroring(),
            prg_rom_banks: MemBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW),
        };
        axrom.into()
    }
}

impl Mapped for Axrom {
    #[inline]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    #[inline]
    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MemMap for Axrom {
    fn map_peek(&self, addr: u16) -> MappedRead {
        if matches!(addr, 0x8000..=0xFFFF) {
            MappedRead::PrgRom(self.prg_rom_banks.translate(addr))
        } else {
            MappedRead::Default
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        if matches!(addr, 0x8000..=0xFFFF) {
            self.prg_rom_banks.set(0, (val & 0x0F).into());
            self.mirroring = if val & Self::SINGLE_SCREEN_B == Self::SINGLE_SCREEN_B {
                Mirroring::SingleScreenB
            } else {
                Mirroring::SingleScreenA
            };
        }
        MappedWrite::Default
    }
}

impl Clock for Axrom {}
impl Regional for Axrom {}
impl Reset for Axrom {}
