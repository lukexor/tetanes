//! `NROM` (Mapper 000)
//!
//! <http://wiki.nesdev.com/w/index.php/NROM>

use crate::{
    cart::Cart,
    common::{Clock, Reset},
    mapper::{Mapped, MappedRead, MappedWrite, Mapper, MemMap},
};
use serde::{Deserialize, Serialize};

const PRG_RAM_SIZE: usize = 8 * 1024;
const CHR_RAM_SIZE: usize = 8 * 1024;

// PPU $0000..=$1FFF 8K Fixed CHR-ROM Bank
// CPU $6000..=$7FFF 2K or 4K PRG-RAM Family Basic only. 8K is provided by default.
// CPU $8000..=$BFFF 16K PRG-ROM Bank 1 for NROM128 or NROM256
// CPU $C000..=$FFFF 16K PRG-ROM Bank 2 for NROM256 or Bank 1 Mirror for NROM128

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nrom {
    mirror_prg_rom: bool,
}

impl Nrom {
    pub fn load(cart: &mut Cart) -> Mapper {
        cart.prg_ram.resize(PRG_RAM_SIZE);
        if cart.chr.is_empty() {
            cart.chr.resize(CHR_RAM_SIZE);
            cart.chr.write_protect(false);
        }
        let nrom = Self {
            mirror_prg_rom: cart.prg_rom.len() <= 0x4000,
        };
        nrom.into()
    }
}

impl MemMap for Nrom {
    fn map_peek(&self, addr: u16) -> MappedRead {
        let addr = addr as usize;
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr),
            0x6000..=0x7FFF => MappedRead::PrgRam(addr & 0x1FFF),
            0x8000..=0xFFFF => MappedRead::PrgRom(if self.mirror_prg_rom {
                addr & 0x3FFF
            } else {
                addr & 0x7FFF
            }),
            _ => MappedRead::None,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        let addr = addr as usize;
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(addr, val),
            0x6000..=0x7FFF => MappedWrite::PrgRam(addr & 0x1FFF, val),
            _ => MappedWrite::None,
        }
    }
}

impl Mapped for Nrom {}
impl Clock for Nrom {}
impl Reset for Nrom {}
