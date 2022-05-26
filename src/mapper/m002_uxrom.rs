//! `UxROM` (Mapper 002)
//!
//! <https://wiki.nesdev.com/w/index.php/UxROM>

use crate::{
    cart::Cart,
    common::{Clocked, Powered},
    mapper::{MapRead, MapWrite, Mapped, MappedRead, MappedWrite, Mapper},
    memory::MemoryBanks,
};
use serde::{Deserialize, Serialize};

const PRG_ROM_WINDOW: usize = 16 * 1024;
const CHR_RAM_SIZE: usize = 8 * 1024;

// PPU $0000..=$1FFF 8K Fixed CHR-ROM Bank
// CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable
// CPU $C000..=$FFFF 16K PRG-ROM Fixed to Last Bank

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Uxrom {
    prg_rom_banks: MemoryBanks,
}

impl Uxrom {
    pub fn load(cart: &mut Cart) -> Mapper {
        if cart.chr.is_empty() {
            cart.chr.resize(CHR_RAM_SIZE);
            cart.chr.write_protect(false);
        }
        let mut uxrom = Self {
            prg_rom_banks: MemoryBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), PRG_ROM_WINDOW),
        };
        let last_bank = uxrom.prg_rom_banks.last();
        uxrom.prg_rom_banks.set(1, last_bank);
        uxrom.into()
    }
}

impl MapRead for Uxrom {
    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr.into()),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::None,
        }
    }
}

impl MapWrite for Uxrom {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(addr.into(), val),
            0x8000..=0xFFFF => {
                self.prg_rom_banks.set(0, val as usize);
                MappedWrite::None
            }
            _ => MappedWrite::None,
        }
    }
}

impl Mapped for Uxrom {}
impl Clocked for Uxrom {}
impl Powered for Uxrom {}
