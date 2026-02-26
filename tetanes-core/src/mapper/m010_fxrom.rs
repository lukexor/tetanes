//! `FxROM`/`MMC4` (Mapper 010).
//!
//! <https://wiki.nesdev.org/w/index.php/MMC4>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sram},
    mapper::{self, Map, Mapper, Mirroring},
    mem::{Banks, Memory},
    ppu::CIRam,
};
use serde::{Deserialize, Serialize};

/// `FxROM`/`MMC4` (Mapper 010).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Fxrom {
    pub chr_rom: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub prg_ram: Memory<Box<[u8]>>,
    pub mirroring: Mirroring,
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
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
    pub latch: [usize; 2],
    pub latch_banks: [u8; 4],
}

impl Fxrom {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_ROM_WINDOW: usize = 4 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024;

    const MIRRORING_MASK: u8 = 0x01;

    /// Load `Fxrom` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let prg_ram = Memory::with_ram_state(Self::PRG_RAM_SIZE, cart.ram_state);
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr_rom.len(), Self::CHR_ROM_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_WINDOW)?;
        let mut fxrom = Self {
            chr_rom,
            prg_rom,
            prg_ram,
            mirroring: cart.mirroring(),
            chr_banks,
            prg_rom_banks,
            latch: [0x00; 2],
            latch_banks: [0x00; 4],
        };
        fxrom.prg_rom_banks.set(1, fxrom.prg_rom_banks.last());
        Ok(fxrom.into())
    }

    pub fn update_banks(&mut self) {
        let bank0 = self.latch_banks[self.latch[0]] as usize;
        let bank1 = self.latch_banks[self.latch[1] + 2] as usize;
        self.chr_banks.set(0, bank0);
        self.chr_banks.set(1, bank1);
    }
}

impl Map for Fxrom {
    // PPU $0000..=$0FFF Two 4K switchable CHR-ROM banks
    // PPU $1000..=$1FFF Two 4K switchable CHR-ROM banks
    // CPU $6000..=$7FFF 8K PRG-RAM bank
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM bank, fixed to the last bank

    /// Read a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_read(&mut self, addr: u16, ciram: &CIRam) -> u8 {
        let val = self.chr_peek(addr, ciram);
        // Update latch after read
        match addr {
            0x0FD8..=0x0FDF | 0x0FE8..=0xFEF | 0x1FD8..=0x1FDF | 0x1FE8..=0x1FEF => {
                let addr = addr as usize;
                self.latch[addr >> 12] = ((addr >> 4) & 0xFF) - 0xFD;
                self.update_banks();
            }
            _ => (),
        }
        val
    }

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_rom[self.chr_banks.translate(addr)],
            0x2000..=0x3EFF => ciram.peek(addr, self.mirroring),
            _ => 0,
        }
    }

    /// Peek a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_peek(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[usize::from(addr & 0x1FFF)],
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => 0,
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[usize::from(addr & 0x1FFF)] = val,
            0xA000..=0xAFFF => {
                self.prg_rom_banks.set(0, (val & 0x0F).into());
            }
            0xB000..=0xEFFF => {
                self.latch_banks[((addr - 0xB000) >> 12) as usize] = val & 0x1F;
                self.update_banks();
            }
            0xF000..=0xFFFF => {
                self.mirroring = match val & Self::MIRRORING_MASK {
                    0b00 => Mirroring::Vertical,
                    _ => Mirroring::Horizontal,
                };
            }
            _ => (),
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Fxrom {
    fn reset(&mut self, _kind: ResetKind) {
        self.latch = [0x00; 2];
        self.latch_banks = [0x00; 4];
        self.update_banks();
    }
}

impl Clock for Fxrom {}
impl Regional for Fxrom {}
impl Sram for Fxrom {}
