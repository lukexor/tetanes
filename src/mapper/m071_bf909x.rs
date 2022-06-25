//! `UNROM` (Mapper 071)
//!
//! <https://wiki.nesdev.org/w/index.php?title=INES_Mapper_071>

use crate::{
    cart::Cart,
    common::{Clock, Reset},
    mapper::{Mapped, MappedRead, MappedWrite, Mapper, MemMap},
    memory::MemoryBanks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

const PRG_ROM_WINDOW: usize = 16 * 1024;
const CHR_RAM_SIZE: usize = 8 * 1024;

const SINGLE_SCREEN_A: u8 = 0x10; // 0b10000

// PPU $0000..=$1FFF 8K Fixed CHR-ROM Banks
// CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable
// CPU $C000..=$FFFF 16K PRG-ROM Fixed to Last Bank

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Bf909Revision {
    Bf909x,
    Bf9097,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bf909x {
    mirroring: Mirroring,
    prg_rom_banks: MemoryBanks,
    variant: Bf909Revision,
}

impl Bf909x {
    pub fn load(cart: &mut Cart) -> Mapper {
        if cart.chr.is_empty() {
            cart.chr.resize(CHR_RAM_SIZE);
            cart.chr.write_protect(false);
        }
        let mut bf909x = Self {
            mirroring: cart.mirroring,
            prg_rom_banks: MemoryBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), PRG_ROM_WINDOW),
            variant: if cart.header.submapper_num == 1 {
                Bf909Revision::Bf9097
            } else {
                Bf909Revision::Bf909x
            },
        };
        bf909x.prg_rom_banks.set(1, bf909x.prg_rom_banks.last());
        bf909x.into()
    }
}

impl Mapped for Bf909x {
    #[inline]
    fn mirroring(&self) -> Option<Mirroring> {
        Some(self.mirroring)
    }
}

impl MemMap for Bf909x {
    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr.into()),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::None,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        // Firehawk uses $9000 to change mirroring
        if addr == 0x9000 {
            self.variant = Bf909Revision::Bf9097;
        }
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(addr.into(), val),
            0x8000..=0xFFFF => {
                if addr >= 0xC000 || self.variant != Bf909Revision::Bf9097 {
                    self.prg_rom_banks.set(0, val as usize);
                } else {
                    self.mirroring = if val & SINGLE_SCREEN_A == SINGLE_SCREEN_A {
                        Mirroring::SingleScreenA
                    } else {
                        Mirroring::SingleScreenB
                    };
                }
                MappedWrite::None
            }
            _ => MappedWrite::None,
        }
    }
}

impl Clock for Bf909x {}
impl Reset for Bf909x {}
