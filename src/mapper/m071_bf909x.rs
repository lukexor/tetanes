//! `UNROM` (Mapper 071)
//!
//! <https://wiki.nesdev.org/w/index.php?title=INES_Mapper_071>

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    mapper::{Mapper, MapperType, Mirroring},
    memory::{BankedMemory, MemRead, MemWrite, RamState},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

const PRG_ROM_WINDOW: usize = 16 * 1024; // 16k ROM
const CHR_WINDOW: usize = 8 * 1024; // 8K ROM/RAM
const CHR_RAM_SIZE: usize = 8 * 1024;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
enum Variant {
    Bf909x,
    Bf9097,
}

#[derive(Debug, Clone)]
#[must_use]
pub struct Bf909x {
    has_chr_ram: bool,
    mirroring: Mirroring,
    // CPU $8000-$BFFF 16 KB PRG ROM Bank Switchable
    // CPU $C000-$FFFF 16 KB PRG ROM Fixed to Last Bank
    prg_rom: BankedMemory,
    chr: BankedMemory, // PPU $0000..=$1FFF 8K Fixed CHR ROM Banks
    variant: Variant,
    open_bus: u8,
}

impl Bf909x {
    pub fn load(cart: Cartridge, state: RamState) -> MapperType {
        let has_chr_ram = cart.chr_rom.is_empty();
        let mut bf909x = Self {
            has_chr_ram,
            mirroring: cart.mirroring(),
            prg_rom: BankedMemory::from(cart.prg_rom, PRG_ROM_WINDOW),
            chr: if has_chr_ram {
                BankedMemory::ram(CHR_RAM_SIZE, CHR_WINDOW, state)
            } else {
                BankedMemory::from(cart.chr_rom, CHR_WINDOW)
            },
            variant: if cart.header.submapper_num == 1 {
                Variant::Bf9097
            } else {
                Variant::Bf909x
            },
            open_bus: 0,
        };
        bf909x.prg_rom.add_bank_range(0x8000, 0xFFFF);
        bf909x.prg_rom.set_bank(0xC000, bf909x.prg_rom.last_bank());
        bf909x.chr.add_bank_range(0x0000, 0x1FFF);
        bf909x.into()
    }
}

impl Mapper for Bf909x {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.open_bus = val;
    }
}

impl MemRead for Bf909x {
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

impl MemWrite for Bf909x {
    fn write(&mut self, addr: u16, val: u8) {
        // Firehawk uses $9000 to change mirroring
        if addr == 0x9000 {
            self.variant = Variant::Bf9097;
        }
        match addr {
            0x0000..=0x1FFF => self.chr.write(addr, val),
            0x8000..=0xFFFF => {
                if addr >= 0xC000 || self.variant != Variant::Bf9097 {
                    let bank = val as usize % self.prg_rom.bank_count();
                    self.prg_rom.set_bank(0x8000, bank);
                } else {
                    self.mirroring = if val & 0x10 > 0 {
                        Mirroring::SingleScreenA
                    } else {
                        Mirroring::SingleScreenB
                    };
                }
            }
            // 0x4020..=0x5FFF // Nothing at this range
            // 0x6000..=0x7FFF // No Save RAM
            _ => (),
        }
    }
}

impl Clocked for Bf909x {}
impl Powered for Bf909x {}

impl Savable for Bf909x {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        if self.has_chr_ram {
            self.chr.save(fh)?;
        }
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        if self.has_chr_ram {
            self.chr.load(fh)?;
        }
        Ok(())
    }
}
