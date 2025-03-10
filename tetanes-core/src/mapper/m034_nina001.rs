//! `NINA-001` (Mapper 034).
//!
//! <https://www.nesdev.org/wiki/NINA-001>

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

/// `NINA-001` (Mapper 034).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nina001 {
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl Nina001 {
    const PRG_ROM_WINDOW: usize = 32 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024;
    const CHR_ROM_WINDOW: usize = 4 * 1024;

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        cart.add_prg_ram(Self::PRG_RAM_SIZE);
        let nina001 = Self {
            chr_banks: Banks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_ROM_WINDOW)?,
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW)?,
        };
        Ok(nina001.into())
    }
}

impl Mirrored for Nina001 {
    fn mirroring(&self) -> Mirroring {
        // hardwired to horizontal
        Mirroring::Horizontal
    }
}

impl MapRead for Nina001 {
    // PPU $0000..=$0FFF 4K switchable CHR ROM bank
    // PPU $1000..=$1FFF 4K switchable CHR ROM bank
    // CPU $8000..=$FFFF 32K switchable PRG ROM bank

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x6000..=0x7FFF => MappedRead::PrgRam(usize::from(addr) & (Self::PRG_RAM_SIZE - 1)),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Bus,
        }
    }
}

impl MapWrite for Nina001 {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => return MappedWrite::ChrRam(self.chr_banks.translate(addr), val),
            0x6000..=0x7FFF => {
                match addr {
                    0x7FFD => self.prg_rom_banks.set(0, (val & 0x01).into()),
                    0x7FFE => self.chr_banks.set(0, (val & 0x0F).into()),
                    0x7FFF => self.chr_banks.set(1, (val & 0x0F).into()),
                    _ => (),
                }
                return MappedWrite::PrgRam(usize::from(addr) & (Self::PRG_RAM_SIZE - 1), val);
            }
            _ => (),
        }
        MappedWrite::Bus
    }
}

impl OnBusRead for Nina001 {}
impl OnBusWrite for Nina001 {}
impl Reset for Nina001 {}
impl Clock for Nina001 {}
impl Regional for Nina001 {}
impl Sram for Nina001 {}
