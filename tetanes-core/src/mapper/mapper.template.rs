//! `Mapper Name` (Mapper NN)
//!
//! <https://www.nesdev.org/wiki/MapperName>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    mapper::{self, Mapped, Mapper, MemMap},
    mem::Banks,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct MapperName {
    pub chr_banks: Banks,
    pub prg_ram_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl MapperName {
    const PRG_WINDOW: usize = /* PRG ROM/RAM bank window */ 0;
    const CHR_WINDOW: usize = /* CHR ROM/RAM bank window */ 0;

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        // Add CHR, PRG, or EX RAM based on mapper
        let mut mapper_name = Self {
            // Registers, Mirroring, etc
            chr_banks: Banks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_WINDOW)?,
            /// Optional PRG RAM
            prg_ram_banks: Banks::new(0x6000, 0x7FFF, cart.prg_ram.len(), Self::PRG_WINDOW)?,
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_WINDOW)?,
        };
        // Set default ROM banks
        Ok(mapper_name.into())
    }

    // Methods to modify banks, clock mapper, etc
}

impl Mapped for MapperName {
    // Implement Mapped methods
}

impl MemMap for MapperName {
    // Memory and banking comment

    /// Implement MemMap methods
}

impl Reset for MapperName {
    /// Optional, Reset methods
}
impl Clock for MapperName {
    /// Optional, Clock methods
}
impl Regional for MapperName {
    /// Optional, Regional methods
}
impl Sram for MapperName {
    /// Optional, Sram methods
}
impl Sample for MapperName {
    /// Optional, Sample methods
}

