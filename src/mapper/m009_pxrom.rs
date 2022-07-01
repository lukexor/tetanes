//! `PxROM`/`MMC2` (Mapper 009)
//!
//! <http://wiki.nesdev.com/w/index.php/MMC2>

use crate::{
    cart::Cart,
    common::{Clock, Kind, Regional, Reset},
    mapper::{Mapped, MappedRead, MappedWrite, Mapper, MemMap, Mirroring},
    mem::MemBanks,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Pxrom {
    mirroring: Mirroring,
    // CHR-ROM $FD/0000 bank select ($B000-$BFFF)
    // CHR-ROM $FE/0000 bank select ($C000-$CFFF)
    // CHR-ROM $FD/1000 bank select ($D000-$DFFF)
    // CHR-ROM $FE/1000 bank select ($E000-$EFFF)
    // 7  bit  0
    // ---- ----
    // xxxC CCCC
    //    | ||||
    //    +-++++- Select 4K CHR-ROM bank for PPU $0000/$1000-$0FFF/$1FFF
    //            used when latch 0/1 = $FD/$FE
    latch: [usize; 2],
    latch_banks: [u8; 4],
    // PPU $0000..=$0FFF Two 4K switchable CHR-ROM banks
    // PPU $1000..=$1FFF Two 4K switchable CHR-ROM banks
    chr_banks: MemBanks,
    // CPU $6000..=$7FFF 8K PRG-RAM bank (PlayChoice version only)
    // CPU $8000..=$9FFF 8K switchable PRG-ROM bank
    // CPU $A000..=$FFFF Three 8K PRG-ROM banks, fixed to the last three banks
    prg_rom_banks: MemBanks,
}

impl Pxrom {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_ROM_WINDOW: usize = 4 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024;

    const MIRRORING_MASK: u8 = 0x01;

    pub fn load(cart: &mut Cart) -> Mapper {
        cart.add_prg_ram(Self::PRG_RAM_SIZE);
        let mut pxrom = Self {
            mirroring: cart.mirroring(),
            latch: [0x00; 2],
            latch_banks: [0x00; 4],
            chr_banks: MemBanks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_ROM_WINDOW),
            prg_rom_banks: MemBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_WINDOW),
        };
        let last_bank = pxrom.prg_rom_banks.last();
        pxrom.prg_rom_banks.set(1, last_bank - 2);
        pxrom.prg_rom_banks.set(2, last_bank - 1);
        pxrom.prg_rom_banks.set(3, last_bank);
        pxrom.into()
    }

    fn update_banks(&mut self) {
        let bank0 = self.latch_banks[self.latch[0]] as usize;
        let bank1 = self.latch_banks[self.latch[1] + 2] as usize;
        self.chr_banks.set(0, bank0);
        self.chr_banks.set(1, bank1);
    }
}

impl Mapped for Pxrom {
    #[inline]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    #[inline]
    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MemMap for Pxrom {
    fn map_read(&mut self, addr: u16) -> MappedRead {
        let val = self.map_peek(addr);
        // Update latch after read
        match addr {
            0x0FD8 | 0x0FE8 | 0x1FD8..=0x1FDF | 0x1FE8..=0x1FEF => {
                let addr = addr as usize;
                self.latch[addr >> 12] = ((addr >> 4) & 0xFF) - 0xFD;
                self.update_banks();
            }
            _ => (),
        }
        val
    }

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Default,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0xA000..=0xAFFF => {
                self.prg_rom_banks.set(0, (val & 0x0F).into());
            }
            0xB000..=0xEFFF => {
                self.latch_banks[((addr - 0xB000) >> 12) as usize] = val & 0x1F;
                self.update_banks();
            }
            0xF000..=0xFFFF => {
                self.mirroring = match val & Self::MIRRORING_MASK {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    _ => unreachable!("impossible mirroring mode"),
                };
            }
            _ => (),
        }
        MappedWrite::Default
    }
}

impl Reset for Pxrom {
    fn reset(&mut self, _kind: Kind) {
        self.latch = [0x00; 2];
        self.latch_banks = [0x00; 4];
        self.update_banks();
    }
}

impl Clock for Pxrom {}
impl Regional for Pxrom {}
