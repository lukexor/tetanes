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
    let cart = Cartridge::from_rom(rom)?;
    match cart.header.mapper_num {
        0 => Ok(Nrom::load(cart)),
        1 => Ok(Sxrom::load(cart)),
        3 => Ok(Cnrom::load(cart)),
        _ => Err(format_err!(
            "unsupported mapper number: {}",
            cart.header.mapper_num
        ))?,
    }
}
