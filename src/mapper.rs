//! NES Mappers
//!
//! http://wiki.nesdev.com/w/index.php/Mapper

use crate::cartridge::Cartridge;
use crate::memory::Memory;
use crate::Result;
use failure::format_err;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use cnrom::Cnrom;
use nrom::Nrom;
use sxrom::Sxrom;

mod cnrom;
mod nrom;
mod sxrom;

pub type MapperRef = Rc<RefCell<Mapper>>;

pub trait Mapper: Memory + Send {
    fn scanline_irq(&self) -> bool;
    fn mirroring(&self) -> Mirroring;
    fn step(&mut self);
}

// http://wiki.nesdev.com/w/index.php/List_of_mappers
#[derive(Debug, Eq, PartialEq)]
enum Board {
    NROM,  // mapper 0, ~51 games - Donkey Kong, Galaga, Pac Man, Super Mario Brothers
    SxROM, // mapper 1:  ~200 games - A Boy and His Blob, Final Fantasy, Metroid, Zelda
    UNROM, // mapper 2, ~82 games - Castlevania, Contra, Mega Man
    CNROM, // mapper 3, ~58 games - Paperboy
    TxROM, // mapper 4, ~175 games - Kickle Cubicle, Krusty's Fun House, Super Mario Brothers 2/3
    AOROM, // mapper 7, ~9 games - Battle Toads, Double Dragon
}

// http://wiki.nesdev.com/w/index.php/Mirroring#Nametable_Mirroring
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleScreen0,
    SingleScreen1,
    FourScreen, // Only ~3 games use 4-screen - maybe implement some day
}

/// Attempts to return a valid Mapper for the given rom.
pub fn load_rom(rom: PathBuf) -> Result<MapperRef> {
    let cartridge = Cartridge::from_rom(rom)?;
    match cartridge.header.mapper_num {
        0 => Ok(Nrom::load(cartridge)),
        1 => Ok(Sxrom::load(cartridge)),
        3 => Ok(Cnrom::load(cartridge)),
        _ => Err(format_err!(
            "unsupported mapper number: {}",
            cartridge.header.mapper_num
        ))?,
    }
}
