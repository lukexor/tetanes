//! CNROM (Mapper 003)
//!
//! <https://wiki.nesdev.com/w/index.php/CNROM>
//! <https://wiki.nesdev.com/w/index.php/INES_Mapper_003>

use crate::{
    cart::Cart,
    common::{Clock, Reset},
    mapper::{MapRead, MapWrite, Mapped, MappedRead, MappedWrite, Mapper},
    memory::MemoryBanks,
};
use serde::{Deserialize, Serialize};

const CHR_ROM_WINDOW: usize = 8 * 1024;

// PPU $0000..=$1FFF 8K CHR-ROM Banks Switchable
// CPU $8000..=$FFFF 16K PRG-ROM Bank Fixed
// CPU $C000..=$FFFF 16K PRG-ROM Bank Fixed or Bank 1 Mirror if only 16 KB PRG-ROM

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Cnrom {
    chr_banks: MemoryBanks,
    mirror_prg: bool,
}

impl Cnrom {
    pub fn load(cart: &mut Cart) -> Mapper {
        let cnrom = Self {
            chr_banks: MemoryBanks::new(0x0000, 0x1FFFF, cart.chr.len(), CHR_ROM_WINDOW),
            mirror_prg: cart.prg_rom.len() <= 0x4000,
        };
        cnrom.into()
    }
}

impl MapRead for Cnrom {
    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x8000..=0xFFFF => MappedRead::PrgRom(if self.mirror_prg {
                addr as usize & 0x3FFF
            } else {
                addr as usize & 0x7fff
            }),
            _ => MappedRead::None,
        }
    }
}

impl MapWrite for Cnrom {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        if matches!(addr, 0x8000..=0xFFFF) {
            self.chr_banks.set(0, (val & 0x03) as usize);
        }
        MappedWrite::None
    }
}

impl Mapped for Cnrom {}
impl Clock for Cnrom {}
impl Reset for Cnrom {}
