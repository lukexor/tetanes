//! `NROM` (Mapper 000).
//!
//! <https://wiki.nesdev.org/w/index.php/NROM>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Map, Mapper},
    memory::Src,
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// `NROM` (Mapper 000).
///
/// The board has no registers at all - the entire cartridge is fixed at power-on - so it holds no
/// state and exists only to configure the page tables in [`Memory::map_prg`]/[`Memory::map_chr`]
/// when the cart is loaded.
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nrom;

impl Nrom {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;

    /// Load `Nrom` from `Cart`.
    // PPU $0000..=$1FFF 8K Fixed CHR-ROM/CHR-RAM Bank
    // CPU $6000..=$7FFF 2K or 4K PRG-RAM Family Basic only. 8K is provided by default.
    // CPU $8000..=$BFFF 16K PRG-ROM Bank 1 for NROM128 or NROM256
    // CPU $C000..=$FFFF 16K PRG-ROM Bank 2 for NROM256 or Bank 1 Mirror for NROM128
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        cart.memory.map_prg(0x6000, 8 * 1024, 0, Src::PrgRam);
        cart.memory
            .map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
        // NROM-128 has a single 16K bank mirrored into both slots, which falls out of the bank
        // index wrapping within the region rather than needing a `mirror_prg_rom` flag.
        cart.memory
            .map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        cart.memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        cart.memory.set_mirroring(cart.mirroring());
        Ok(Self.into())
    }
}

impl Map for Nrom {
    fn uses_page_tables(&self) -> bool {
        true
    }

    fn mirroring(&self) -> Mirroring {
        // Hard-wired by the cart at load; the page tables hold the real mapping.
        Mirroring::default()
    }

    fn chr_peek(&self, _addr: u16, _ciram: &CIRam) -> u8 {
        unreachable!("NROM reads are served from page tables")
    }

    fn prg_peek(&self, _addr: u16) -> u8 {
        unreachable!("NROM reads are served from page tables")
    }
}

impl Reset for Nrom {}
impl Clock for Nrom {}
impl Regional for Nrom {}
impl Sram for Nrom {}
