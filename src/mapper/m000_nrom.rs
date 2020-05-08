//! NROM (mapper 0)
//!
//! [http://wiki.nesdev.com/w/index.php/NROM]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    mapper::{Mapper, MapperType, Mirroring},
    memory::{BankedMemory, MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

const PRG_RAM_WINDOW: usize = 8 * 1024;
const PRG_ROM_WINDOW: usize = 16 * 1024;
const CHR_WINDOW: usize = 8 * 1024;
const PRG_RAM_SIZE: usize = 8 * 1024;
const CHR_RAM_SIZE: usize = 8 * 1024;

/// NROM
#[derive(Debug, Clone)]
pub struct Nrom {
    has_chr_ram: bool,
    battery_backed: bool,
    mirroring: Mirroring,
    prg_ram: BankedMemory, // CPU $6000-$7FFF 2K or 4K PRG RAM Family Basic only. 8K is provided
    // CPU $8000-$BFFF 16 KB PRG ROM Bank 1 for NROM128 or NROM256
    // CPU $C000-$FFFF 16 KB PRG ROM Bank 2 for NROM256 or Bank 1 Mirror for NROM128
    prg_rom: BankedMemory,
    chr: BankedMemory, // PPU $0000..=$1FFFF 8K Fixed CHR ROM Bank
    open_bus: u8,
}

impl Nrom {
    pub fn load(cart: Cartridge) -> MapperType {
        let has_chr_ram = cart.chr_rom.is_empty();
        let mut nrom = Self {
            has_chr_ram,
            battery_backed: cart.battery_backed(),
            mirroring: cart.mirroring(),
            prg_ram: BankedMemory::ram(PRG_RAM_SIZE, PRG_RAM_WINDOW),
            prg_rom: BankedMemory::from(cart.prg_rom, PRG_ROM_WINDOW),
            chr: if has_chr_ram {
                BankedMemory::ram(CHR_RAM_SIZE, CHR_WINDOW)
            } else {
                BankedMemory::from(cart.chr_rom, CHR_WINDOW)
            },
            open_bus: 0x00,
        };
        nrom.prg_ram.add_bank_range(0x6000, 0x7FFF);
        nrom.prg_rom.add_bank_range(0x8000, 0xFFFF);
        if nrom.prg_rom.len() <= 0x4000 {
            // NROM128 mirrors upper bank
            nrom.prg_rom.set_bank_mirror(0xC000, 0);
        }
        nrom.chr.add_bank_range(0x0000, 0x1FFF);
        nrom.into()
    }
}

impl Mapper for Nrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn battery_backed(&self) -> bool {
        self.battery_backed
    }
    fn save_sram<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        if self.battery_backed {
            self.prg_ram.save(fh)?;
        }
        Ok(())
    }
    fn load_sram<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        if self.battery_backed {
            self.prg_ram.load(fh)?;
        }
        Ok(())
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Nrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            // PPU 8K Fixed CHR bank
            0x0000..=0x1FFF => self.chr.peek(addr),
            0x6000..=0x7FFF => self.prg_ram.peek(addr),
            0x8000..=0xFFFF => self.prg_rom.peek(addr),
            // 0x4020..=0x5FFF Nothing at this range
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Nrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            // Only CHR-RAM can be written to
            0x0000..=0x1FFF if self.has_chr_ram => self.chr.write(addr, val),
            0x6000..=0x7FFF => self.prg_ram.write(addr, val),
            // 0x4020..=0x5FFF Nothing at this range
            // 0x8000..=0xFFFF ROM is write-only
            _ => (),
        }
    }
}

impl Clocked for Nrom {}
impl Powered for Nrom {}

impl Savable for Nrom {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.has_chr_ram.save(fh)?;
        self.battery_backed.save(fh)?;
        self.mirroring.save(fh)?;
        self.open_bus.save(fh)?;
        self.prg_ram.save(fh)?;
        self.prg_rom.save(fh)?;
        self.chr.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.has_chr_ram.load(fh)?;
        self.battery_backed.load(fh)?;
        self.mirroring.load(fh)?;
        self.open_bus.load(fh)?;
        self.prg_ram.load(fh)?;
        self.prg_rom.load(fh)?;
        self.chr.load(fh)?;
        Ok(())
    }
}
