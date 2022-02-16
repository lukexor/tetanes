//! `PxROM`/`MMC2` (mapper 9)
//!
//! <http://wiki.nesdev.com/w/index.php/MMC2>

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    mapper::{Mapper, MapperType, Mirroring},
    memory::{BankedMemory, MemRead, MemWrite, RamState},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

const PRG_WINDOW: usize = 8 * 1024;
const CHR_ROM_WINDOW: usize = 4 * 1024;
const PRG_RAM_SIZE: usize = 8 * 1024;

#[derive(Debug, Clone)]
#[must_use]
pub struct Pxrom {
    mirroring: Mirroring,
    // CHR ROM $FD/0000 bank select ($B000-$BFFF)
    // CHR ROM $FE/0000 bank select ($C000-$CFFF)
    // CHR ROM $FD/1000 bank select ($D000-$DFFF)
    // CHR ROM $FE/1000 bank select ($E000-$EFFF)
    // 7  bit  0
    // ---- ----
    // xxxC CCCC
    //    | ||||
    //    +-++++- Select 4 KB CHR ROM bank for PPU $0000/$1000-$0FFF/$1FFF
    //            used when latch 0/1 = $FD/$FE
    chr_rom_banks: [usize; 4], // Banks for latch 0 and latch 1
    latch: [usize; 2],
    prg_ram: BankedMemory, // CPU $6000-$7FFF 8 KB PRG RAM bank (PlayChoice version only)
    // CPU $8000-$9FFF 8 KB switchable PRG ROM bank
    // CPU $A000-$FFFF Three 8 KB PRG ROM banks, fixed to the last three banks
    prg_rom: BankedMemory,
    // PPU $0000..=$0FFF Two 4 KB switchable CHR ROM banks
    // PPU $1000..=$1FFF Two 4 KB switchable CHR ROM banks
    chr_rom: BankedMemory,
    open_bus: u8,
}

impl Pxrom {
    pub fn load(cart: Cartridge, state: RamState) -> MapperType {
        let mut pxrom = Self {
            mirroring: cart.mirroring(),
            chr_rom_banks: [0x00; 4],
            latch: [0x00; 2],
            prg_ram: BankedMemory::ram(PRG_RAM_SIZE, PRG_WINDOW, state),
            prg_rom: BankedMemory::from(cart.prg_rom, PRG_WINDOW),
            chr_rom: BankedMemory::from(cart.chr_rom, CHR_ROM_WINDOW),
            open_bus: 0x00,
        };
        pxrom.prg_ram.add_bank(0x6000, 0x7FFF);
        pxrom.prg_rom.add_bank_range(0x8000, 0xFFFF);
        let last_bank = pxrom.prg_rom.last_bank();
        pxrom.prg_rom.set_bank(0xA000, last_bank - 2);
        pxrom.prg_rom.set_bank(0xC000, last_bank - 1);
        pxrom.prg_rom.set_bank(0xE000, last_bank);
        pxrom.chr_rom.add_bank_range(0x0000, 0x1FFF);
        pxrom.into()
    }

    fn update_banks(&mut self) {
        self.chr_rom
            .set_bank(0x0000, self.chr_rom_banks[self.latch[0]]);
        self.chr_rom
            .set_bank(0x1000, self.chr_rom_banks[2 + self.latch[1]]);
    }
}

impl Mapper for Pxrom {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Pxrom {
    fn read(&mut self, addr: u16) -> u8 {
        let val = self.peek(addr);
        match addr {
            0x0FD8 | 0x0FE8 | 0x1FD8..=0x1FDF | 0x1FE8..=0x1FEF => {
                let latch = (addr >> 12) as usize;
                self.latch[latch] = ((addr as usize >> 4) & 0xFF) - 0xFD;
                self.update_banks();
            }
            _ => (),
        }
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_rom.peek(addr),
            0x6000..=0x7FFF => self.prg_ram.peek(addr),
            0x8000..=0xFFFF => self.prg_rom.peek(addr),
            // 0x4020..=0x5FFF Nothing at this range
            _ => self.open_bus,
        }
    }
}

impl MemWrite for Pxrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x6000..=0x7FFF => self.prg_ram.write(addr, val),
            0xA000..=0xAFFF => self.prg_rom.set_bank(0x8000, (val & 0x0F) as usize),
            0xB000..=0xEFFF => {
                let bank = ((addr - 0xB000) >> 12) as usize;
                self.chr_rom_banks[bank] = (val & 0x1F) as usize;
                self.update_banks();
            }
            0xF000..=0xFFFF => {
                self.mirroring = match val & 0x01 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    _ => unreachable!("impossible mirroring mode"),
                }
            }
            // 0x0000..=0x1FFF ROM is write-only
            // 0x4020..=0x5FFF Nothing at this range
            // 0x8000..=0x9FFF ROM is write-only
            _ => (),
        }
    }
}

impl Clocked for Pxrom {}

impl Powered for Pxrom {
    fn reset(&mut self) {
        self.chr_rom_banks = [0x00; 4];
        self.latch = [0x00; 2];
        self.update_banks();
    }
}

impl Savable for Pxrom {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.mirroring.save(fh)?;
        self.chr_rom_banks.save(fh)?;
        self.latch.save(fh)?;
        self.prg_ram.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.mirroring.load(fh)?;
        self.chr_rom_banks.load(fh)?;
        self.latch.load(fh)?;
        self.update_banks();
        self.prg_ram.load(fh)?;
        Ok(())
    }
}
