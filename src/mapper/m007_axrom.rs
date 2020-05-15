//!AExROM (Mapper 7)
//!
//! [https://wiki.nesdev.com/w/index.php/AxROM]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    mapper::{Mapper, MapperType, Mirroring},
    memory::{BankedMemory, MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

const PRG_ROM_WINDOW: usize = 32 * 1024;
const CHR_WINDOW: usize = 8 * 1024;
const CHR_RAM_SIZE: usize = 8 * 1024;

/// AxROM
#[derive(Debug, Clone)]
pub struct Axrom {
    has_chr_ram: bool,
    mirroring: Mirroring,
    prg_rom: BankedMemory, // CPU $8000..=$FFFF 32 KB switchable PRG ROM bank
    chr: BankedMemory,     // PPU $0000..=$1FFF 8KB CHR ROM/RAM Bank Fixed
    open_bus: u8,
}

impl Axrom {
    pub fn load(cart: Cartridge) -> MapperType {
        let has_chr_ram = cart.chr_rom.is_empty();
        let mut axrom = Self {
            has_chr_ram,
            mirroring: cart.mirroring(),
            prg_rom: BankedMemory::from(cart.prg_rom, PRG_ROM_WINDOW),
            chr: if has_chr_ram {
                BankedMemory::ram(CHR_RAM_SIZE, CHR_WINDOW)
            } else {
                BankedMemory::from(cart.chr_rom, CHR_WINDOW)
            },
            open_bus: 0,
        };
        axrom.prg_rom.add_bank_range(0x8000, 0xFFFF);
        let last_bank = axrom.prg_rom.last_bank();
        axrom.prg_rom.set_bank(0x8000, last_bank);
        axrom.chr.add_bank_range(0x0000, 0x1FFF);
        axrom.into()
    }
}

impl Mapper for Axrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Axrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr.peek(addr),
            0x8000..=0xFFFF => self.prg_rom.peek(addr),
            // 0x4020..=0x5FFF Nothing at this range
            // 0x6000..=0x7FFF Nothing at this range
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Axrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF if self.has_chr_ram => self.chr.write(addr, val),
            0x8000..=0xFFFF => {
                let bank = (val & 0x07) as usize;
                let bank_count = self.prg_rom.bank_count();
                self.prg_rom.set_bank(0x8000, bank % bank_count);
                self.mirroring = if val & 0x10 == 0x10 {
                    Mirroring::SingleScreenB
                } else {
                    Mirroring::SingleScreenA
                };
            }
            // 0x4020..=0x7FFF Nothing at this range
            _ => (),
        }
    }
}

impl Clocked for Axrom {}
impl Powered for Axrom {}

impl Savable for Axrom {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.mirroring.save(fh)?;
        self.prg_rom.save(fh)?;
        if self.has_chr_ram {
            self.chr.save(fh)?;
        }
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.mirroring.load(fh)?;
        self.prg_rom.load(fh)?;
        if self.has_chr_ram {
            self.chr.load(fh)?;
        }
        Ok(())
    }
}
