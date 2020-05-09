//! CNROM (Mapper 3)
//!
//! [https://wiki.nesdev.com/w/index.php/CNROM]()
//! [https://wiki.nesdev.com/w/index.php/INES_Mapper_003]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    mapper::{Mapper, MapperType, Mirroring},
    memory::{BankedMemory, MemRead, MemWrite},
    serialization::Savable,
};

const PRG_ROM_WINDOW: usize = 16 * 1024;
const CHR_ROM_WINDOW: usize = 8 * 1024;

/// CNROM
#[derive(Debug, Clone)]
pub struct Cnrom {
    mirroring: Mirroring,
    // CPU $8000-$FFFF 16 KB PRG ROM Bank 1 Fixed
    // CPU $C000-$FFFF 16 KB PRG ROM Bank 2 Fixed or Bank 1 Mirror if only 16 KB PRG ROM
    prg_rom: BankedMemory,
    chr_rom: BankedMemory, // PPU $0000..=$1FFF 8K CHR ROM Banks Switchable
    open_bus: u8,
}

impl Cnrom {
    pub fn load(cart: Cartridge) -> MapperType {
        let mut cnrom = Self {
            mirroring: cart.mirroring(),
            prg_rom: BankedMemory::from(cart.prg_rom, PRG_ROM_WINDOW),
            chr_rom: BankedMemory::from(cart.chr_rom, CHR_ROM_WINDOW),
            open_bus: 0,
        };
        cnrom.prg_rom.add_bank_range(0x8000, 0xFFFF);
        if cnrom.prg_rom.len() <= 0x4000 {
            // Mirrors lower bank
            cnrom.prg_rom.set_bank_mirror(0xC000, 0);
        }
        cnrom.chr_rom.add_bank_range(0x0000, 0x1FFF);
        cnrom.into()
    }
}

impl Mapper for Cnrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Cnrom {
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

impl MemWrite for Cnrom {
    fn write(&mut self, addr: u16, val: u8) {
        if let 0x8000..=0xFFFF = addr {
            self.chr_rom.set_bank(0x0000, val as usize & 3);
        }
        // 0x0000..=0x1FFF ROM is write-only
        // 0x4020..=0x5FFF Nothing at this range
        // 0x6000..=0x7FFF No Save RAM
    }
}

impl Clocked for Cnrom {}
impl Powered for Cnrom {}
impl Savable for Cnrom {}
