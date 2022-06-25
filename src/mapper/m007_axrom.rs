//! `AxROM` (Mapper 007)
//!
//! <https://wiki.nesdev.com/w/index.php/AxROM>

use crate::{
    cart::Cart,
    common::{Clock, Reset},
    mapper::{MapRead, MapWrite, Mapped, MappedRead, MappedWrite, Mapper},
    memory::MemoryBanks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

const PRG_ROM_WINDOW: usize = 32 * 1024;
const CHR_RAM_SIZE: usize = 8 * 1024;

const SINGLE_SCREEN_B: u8 = 0x10; // 0b10000

// PPU $0000..=$1FFF 8K CHR-ROM/RAM Bank Fixed
// CPU $8000..=$FFFF 32K switchable PRG-ROM bank

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Axrom {
    mirroring: Mirroring,
    prg_rom_banks: MemoryBanks,
}

impl Axrom {
    pub fn load(cart: &mut Cart) -> Mapper {
        cart.chr.resize(CHR_RAM_SIZE);
        cart.chr.write_protect(false);
        let axrom = Self {
            mirroring: cart.mirroring,
            prg_rom_banks: MemoryBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), PRG_ROM_WINDOW),
        };
        axrom.into()
    }
}

impl Mapped for Axrom {
    #[inline]
    fn mirroring(&self) -> Option<Mirroring> {
        Some(self.mirroring)
    }
}

impl MapRead for Axrom {
    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr.into()),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::None,
        }
    }
}

impl MapWrite for Axrom {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(addr.into(), val),
            0x8000..=0xFFFF => {
                self.prg_rom_banks.set(0, (val & 0x0F) as usize);
                self.mirroring = if val & SINGLE_SCREEN_B == SINGLE_SCREEN_B {
                    Mirroring::SingleScreenB
                } else {
                    Mirroring::SingleScreenA
                };
                MappedWrite::None
            }
            _ => MappedWrite::None,
        }
    }
}

impl Clock for Axrom {}
impl Reset for Axrom {}
