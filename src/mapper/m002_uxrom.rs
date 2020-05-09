//! UxROM (Mapper 2)
//!
//! [https://wiki.nesdev.com/w/index.php/UxROM]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    mapper::{Mapper, MapperType, Mirroring},
    memory::{BankedMemory, MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

const PRG_ROM_WINDOW: usize = 16 * 1024; // 16k ROM
const CHR_WINDOW: usize = 8 * 1024; // 8K ROM/RAM
const CHR_RAM_SIZE: usize = 8 * 1024;

/// UxROM
#[derive(Debug, Clone)]
pub struct Uxrom {
    mirroring: Mirroring,
    // CPU $8000-$BFFF 16 KB PRG ROM Bank Switchable
    // CPU $C000-$FFFF 16 KB PRG ROM Fixed to Last Bank
    prg_rom: BankedMemory,
    chr: BankedMemory, // PPU $0000..=$1FFF 8K Fixed CHR ROM Banks
    open_bus: u8,
}

impl Uxrom {
    pub fn load(cart: Cartridge) -> MapperType {
        let mut uxrom = Self {
            mirroring: cart.mirroring(),
            prg_rom: BankedMemory::from(cart.prg_rom, PRG_ROM_WINDOW),
            chr: if cart.chr_rom.is_empty() {
                BankedMemory::ram(CHR_RAM_SIZE, CHR_WINDOW)
            } else {
                BankedMemory::from(cart.chr_rom, CHR_WINDOW)
            },
            open_bus: 0,
        };
        uxrom.prg_rom.add_bank_range(0x8000, 0xFFFF);
        uxrom.prg_rom.set_bank(0xC000, uxrom.prg_rom.last_bank());
        uxrom.chr.add_bank_range(0x0000, 0x1FFF);
        uxrom.into()
    }
}

impl Mapper for Uxrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Uxrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr.peek(addr),
            0x8000..=0xFFFF => self.prg_rom.peek(addr),
            // 0x4020..=0x5FFF Nothing at this range
            // 0x6000..=0x7FFF No Save RAM
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Uxrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => self.chr.write(addr, val),
            0x8000..=0xFFFF => {
                let bank = val as usize % self.prg_rom.bank_count();
                self.prg_rom.set_bank(0x8000, bank);
            }
            // 0x4020..=0x5FFF // Nothing at this range
            // 0x6000..=0x7FFF // No Save RAM
            _ => (),
        }
    }
}

impl Clocked for Uxrom {}
impl Powered for Uxrom {}

impl Savable for Uxrom {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.mirroring.save(fh)?;
        self.prg_rom.save(fh)?;
        self.chr.save(fh)?;
        self.open_bus.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.mirroring.load(fh)?;
        self.prg_rom.load(fh)?;
        self.chr.load(fh)?;
        self.open_bus.load(fh)?;
        Ok(())
    }
}
