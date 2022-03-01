//! `GxROM` (Mapper 066)
//!
//! <https://wiki.nesdev.org/w/index.php?title=GxROM>

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    mapper::{Mapper, MapperType, Mirroring},
    memory::{BankedMemory, MemRead, MemWrite},
    serialization::Savable,
};

const PRG_ROM_WINDOW: usize = 32 * 1024; // 32k ROM
const CHR_WINDOW: usize = 8 * 1024; // 8K ROM

#[derive(Debug, Clone)]
#[must_use]
pub struct Gxrom {
    mirroring: Mirroring,
    // CPU $8000-$FFFF 32 KB PRG ROM Bank Switchable
    prg_rom: BankedMemory,
    chr_rom: BankedMemory, // PPU $0000..=$1FFF 8K Fixed CHR ROM Banks
    open_bus: u8,
}

impl Gxrom {
    pub fn load(cart: Cartridge) -> MapperType {
        let mut gxrom = Self {
            mirroring: cart.mirroring(),
            prg_rom: BankedMemory::from(cart.prg_rom, PRG_ROM_WINDOW),
            chr_rom: BankedMemory::from(cart.chr_rom, CHR_WINDOW),
            open_bus: 0,
        };
        gxrom.prg_rom.add_bank_range(0x8000, 0xFFFF);
        gxrom.chr_rom.add_bank_range(0x0000, 0x1FFF);
        gxrom.into()
    }
}

impl Mapper for Gxrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Gxrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_rom.peek(addr),
            0x8000..=0xFFFF => self.prg_rom.peek(addr),
            // 0x4020..=0x5FFF Nothing at this range
            // 0x6000..=0x7FFF No Save RAM
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Gxrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => self.chr_rom.write(addr, val),
            0x8000..=0xFFFF => {
                self.chr_rom.set_bank(0x0000, (val & 0x0F) as usize);
                self.prg_rom.set_bank(0x8000, ((val & 0x30) >> 4) as usize);
            }
            // 0x4020..=0x5FFF // Nothing at this range
            // 0x6000..=0x7FFF // No Save RAM
            _ => (),
        }
    }
}

impl Clocked for Gxrom {}
impl Powered for Gxrom {}
impl Savable for Gxrom {}
