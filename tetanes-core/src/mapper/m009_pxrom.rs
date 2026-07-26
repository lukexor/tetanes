//! `PxROM/MMC2 (Mapper 009)`.
//!
//! <https://wiki.nesdev.org/w/index.php/MMC2>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `PxROM/MMC2 (Mapper 009)`.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Pxrom {
    pub mirroring: Mirroring,
    pub prg_bank: u8,
    /// Which of the two bank registers each 4K CHR half is currently using, flipped by reads of
    /// the $FD/$FE tile addresses.
    pub latch: [usize; 2],
    /// $FD/$FE bank registers for the low half, then for the high half.
    pub latch_banks: [u8; 4],
}

impl Pxrom {
    const CHR_WINDOW: usize = 4 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const MIRRORING_MASK: u8 = 0x01;
    const PRG_WINDOW: usize = 8 * 1024;

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            prg_bank: 0,
            latch: [0; 2],
            latch_banks: [0; 4],
        };
        board.sync(&mut cart.memory);
        Ok(board.into())
    }

    /// Re-map one 4K CHR window from whichever bank register its latch currently selects.
    ///
    /// The latch flips on tile fetches, so this runs thousands of times a frame; rebuilding every
    /// page table entry through `sync` for it cost Punch-Out!! ~20% of its frame time.
    fn sync_chr(&self, memory: &mut Memory, half: usize) {
        let bank = self.latch_banks[self.latch[half] + half * 2];
        memory.map_chr(
            (half * Self::CHR_WINDOW) as u16,
            Self::CHR_WINDOW,
            i32::from(bank),
            Src::Chr,
        );
    }
}

impl Map for Pxrom {
    fn uses_page_tables(&self) -> bool {
        true
    }

    /// The CHR latch is driven by which tile addresses the PPU fetches.
    fn watches_ppu_bus(&self) -> bool {
        true
    }

    fn ppu_bus_addr(&mut self, memory: &mut Memory, addr: u16) {
        if matches!(addr, 0x0FD8 | 0x0FE8 | 0x1FD8..=0x1FDF | 0x1FE8..=0x1FEF) {
            let addr = addr as usize;
            let half = addr >> 12;
            let latch = ((addr >> 4) & 0xFF) - 0xFD;
            if self.latch[half] != latch {
                self.latch[half] = latch;
                self.sync_chr(memory, half);
            }
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr {
            0xA000..=0xAFFF => self.prg_bank = val & 0x0F,
            0xB000..=0xEFFF => self.latch_banks[((addr - 0xB000) >> 12) as usize] = val & 0x1F,
            0xF000..=0xFFFF => {
                self.mirroring = match val & Self::MIRRORING_MASK {
                    0b00 => Mirroring::Vertical,
                    _ => Mirroring::Horizontal,
                };
            }
            _ => return,
        }
        self.sync(memory);
    }

    fn sync(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_prg(0xA000, Self::PRG_WINDOW, -3, Src::PrgRom);
        memory.map_prg(0xC000, Self::PRG_WINDOW, -2, Src::PrgRom);
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
        self.sync_chr(memory, 0);
        self.sync_chr(memory, 1);
        memory.set_mirroring(self.mirroring);
    }
}

impl Reset for Pxrom {}
impl Clock for Pxrom {}
impl Regional for Pxrom {}
impl Sram for Pxrom {}
