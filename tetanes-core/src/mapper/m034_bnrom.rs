//! `BNROM` (Mapper 034)
//!
//! <https://www.nesdev.org/wiki/BNROM>

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
pub struct Bnrom {
    pub mirroring: Mirroring,
    pub prg_rom_banks: MemBanks,
}

impl Bnrom {
    const PRG_ROM_WINDOW: usize = 0x8000;
    const CHR_RAM_SIZE: usize = 0x1000;
    const SINGLE_SCREEN_B: u8 = 0b10000;

    pub fn load(cart: &mut Cart) -> Mapper {
        if !cart.has_chr_rom() && cart.chr_ram.is_empty() {
            cart.add_chr_ram(Self::CHR_RAM_SIZE);
        }
        let bnrom = Self {
            mirroring: cart.mirroring(),
            prg_rom_banks: MemBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW),
        };
        bnrom.into()
    }
}

impl Mapped for Bnrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MemMap for Bnrom {
    // PPU $0000..=$1FFF 8K CHR-RAM Bank Fixed
    // CPU $8000..=$FFFF 32K switchable PRG-ROM bank

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr.into()),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::PpuRam,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(addr.into(), val),
            0x8000..=0xFFFF => {
                self.prg_rom_banks.set(0, (val & 0x0F).into());
                self.mirroring = if val & Self::SINGLE_SCREEN_B == Self::SINGLE_SCREEN_B {
                    Mirroring::SingleScreenB
                } else {
                    Mirroring::SingleScreenA
                };
                MappedWrite::PpuRam
            }
            _ => MappedWrite::PpuRam,
        }
    }
}

impl Clock for Bnrom {}
impl Regional for Bnrom {}
impl Reset for Bnrom {}