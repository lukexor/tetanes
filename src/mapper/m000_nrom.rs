//! `NROM` (Mapper 000)
//!
//! <http://wiki.nesdev.com/w/index.php/NROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset},
    mapper::{Mapped, MappedRead, Mapper, MemMap},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

// PPU $0000..=$1FFF 8K Fixed CHR-ROM Bank
// CPU $6000..=$7FFF 2K or 4K PRG-RAM Family Basic only. 8K is provided by default.
// CPU $8000..=$BFFF 16K PRG-ROM Bank 1 for NROM128 or NROM256
// CPU $C000..=$FFFF 16K PRG-ROM Bank 2 for NROM256 or Bank 1 Mirror for NROM128

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nrom {
    mirroring: Mirroring,
    mirror_prg_rom: bool,
}

impl Nrom {
    const PRG_RAM_SIZE: usize = 8 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    pub fn load(cart: &mut Cart) -> Mapper {
        // Family Basic supported 2-4K of PRG-RAM, but we'll provide 8K by default.
        cart.add_prg_ram(Self::PRG_RAM_SIZE);
        // NROM doesn't have CHR-RAM - but a lot of homebrew games use Mapper 000 with CHR-RAM, so
        // we'll provide some.
        if !cart.has_chr() {
            cart.add_chr_ram(Self::CHR_RAM_SIZE);
        };
        let nrom = Self {
            mirroring: cart.mirroring(),
            mirror_prg_rom: cart.prg_rom.len() <= 0x4000,
        };
        nrom.into()
    }
}

impl MemMap for Nrom {
    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(addr.into()),
            0x8000..=0xFFFF => {
                let mirror = if self.mirror_prg_rom { 0x3FFF } else { 0x7FFF };
                MappedRead::PrgRom((addr & mirror).into())
            }
            _ => MappedRead::None,
        }
    }
}

impl Mapped for Nrom {
    #[inline]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    #[inline]
    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl Clock for Nrom {}
impl Regional for Nrom {}
impl Reset for Nrom {}
