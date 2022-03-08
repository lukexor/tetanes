//! `GxROM` (Mapper 066)
//!
//! <https://wiki.nesdev.org/w/index.php?title=GxROM>

use crate::{
    cart::Cart,
    common::{Clocked, Powered},
    mapper::{MapRead, MapWrite, Mapped, MappedRead, MappedWrite, Mapper},
    memory::MemoryBanks,
};

const PRG_ROM_WINDOW: usize = 32 * 1024;
const CHR_WINDOW: usize = 8 * 1024;

const CHR_BANK_MASK: u8 = 0x0F; // 0b1111
const PRG_BANK_MASK: u8 = 0x30; // 0b110000

// PPU $0000..=$1FFF 8K CHR-ROM Bank Switchable
// CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable

#[derive(Debug, Clone)]
#[must_use]
pub struct Gxrom {
    chr_banks: MemoryBanks,
    prg_rom_banks: MemoryBanks,
}

impl Gxrom {
    pub fn load(cart: &mut Cart) -> Mapper {
        let gxrom = Self {
            chr_banks: MemoryBanks::new(0x0000, 0x1FFF, cart.chr.len(), CHR_WINDOW),
            prg_rom_banks: MemoryBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), PRG_ROM_WINDOW),
        };
        gxrom.into()
    }
}

impl MapRead for Gxrom {
    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::None,
        }
    }
}

impl MapWrite for Gxrom {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(self.chr_banks.translate(addr), val),
            0x8000..=0xFFFF => {
                self.chr_banks.set(0, (val & CHR_BANK_MASK) as usize);
                self.prg_rom_banks
                    .set(0, ((val & PRG_BANK_MASK) >> 4) as usize);
                MappedWrite::None
            }
            _ => MappedWrite::None,
        }
    }
}

impl Mapped for Gxrom {}
impl Clocked for Gxrom {}
impl Powered for Gxrom {}
