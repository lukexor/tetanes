//! NES Memory Mappers for Cartridges
//!
//! [http://wiki.nesdev.com/w/index.php/Mapper]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::memory::Memory;
use crate::serialization::Savable;
use crate::util::Result;
use failure::format_err;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::rc::Rc;

use cnrom::Cnrom;
use nrom::Nrom;
use sxrom::Sxrom;
use txrom::Txrom;
use uxrom::Uxrom;

pub mod cnrom;
pub mod nrom;
pub mod sxrom;
pub mod txrom;
pub mod uxrom;

/// Alias for Mapper wrapped in a Rc/RefCell
pub type MapperRef = Rc<RefCell<Mapper>>;

/// Mapper trait requiring Memory + Send + Savable
pub trait Mapper: Memory + Send + Savable {
    fn irq_pending(&mut self) -> bool;
    fn mirroring(&self) -> Mirroring;
    fn clock(&mut self, ppu: &Ppu);
    fn cart(&self) -> &Cartridge;
    fn cart_mut(&mut self) -> &mut Cartridge;
}

/// Attempts to return a valid Mapper for the given rom.
pub fn load_rom(rom: PathBuf) -> Result<MapperRef> {
    let cart = Cartridge::from_rom(rom)?;
    match cart.header.mapper_num {
        0 => Ok(Nrom::load(cart)),
        1 => Ok(Sxrom::load(cart)),
        2 => Ok(Uxrom::load(cart)),
        3 => Ok(Cnrom::load(cart)),
        4 => Ok(Txrom::load(cart)),
        _ => Err(format_err!(
            "unsupported mapper number: {}",
            cart.header.mapper_num
        ))?,
    }
}

/// Nametable Mirroring Mode
///
/// [http://wiki.nesdev.com/w/index.php/Mirroring#Nametable_Mirroring]()
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleScreen0,
    SingleScreen1,
    FourScreen, // Only ~3 games use 4-screen - maybe implement some day
}

impl Savable for Mirroring {
    fn save(&self, fh: &mut Write) -> Result<()> {
        (*self as u8).save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => Mirroring::Horizontal,
            1 => Mirroring::Vertical,
            2 => Mirroring::SingleScreen0,
            3 => Mirroring::SingleScreen1,
            4 => Mirroring::FourScreen,
            _ => panic!("invalid Mirroring value"),
        };
        Ok(())
    }
}
