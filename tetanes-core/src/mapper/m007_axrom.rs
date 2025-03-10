//! `AxROM` (Mapper 007).
//!
//! <https://wiki.nesdev.com/w/index.php/AxROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{
        self, MapRead, MapWrite, MappedRead, MappedWrite, Mapper, Mirrored, OnBusRead, OnBusWrite,
    },
    mem::Banks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `AxROM` (Mapper 007).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Axrom {
    pub mirroring: Mirroring,
    pub prg_rom_banks: Banks,
}

impl Axrom {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;
    const SINGLE_SCREEN_B: u8 = 0x10; // 0b10000

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        if !cart.has_chr_rom() && cart.chr_ram.is_empty() {
            cart.add_chr_ram(Self::CHR_RAM_SIZE);
        }
        let axrom = Self {
            mirroring: cart.mirroring(),
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW)?,
        };
        Ok(axrom.into())
    }
}

impl Mirrored for Axrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MapRead for Axrom {
    // PPU $0000..=$1FFF 8K CHR-RAM Bank Fixed
    // CPU $8000..=$FFFF 32K switchable PRG-ROM bank

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr.into()),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Bus,
        }
    }
}

impl MapWrite for Axrom {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => MappedWrite::ChrRam(addr.into(), val),
            0x8000..=0xFFFF => {
                self.prg_rom_banks.set(0, (val & 0x0F).into());
                self.mirroring = if val & Self::SINGLE_SCREEN_B == Self::SINGLE_SCREEN_B {
                    Mirroring::SingleScreenB
                } else {
                    Mirroring::SingleScreenA
                };
                MappedWrite::Bus
            }
            _ => MappedWrite::Bus,
        }
    }
}

impl OnBusRead for Axrom {}
impl OnBusWrite for Axrom {}
impl Reset for Axrom {}
impl Clock for Axrom {}
impl Regional for Axrom {}
impl Sram for Axrom {}
