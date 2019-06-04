//! NES Memory Mappers for Cartridges
//!
//! [http://wiki.nesdev.com/w/index.php/Mapper]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::memory::{Banks, Memory, Ram, Rom};
use crate::serialization::Savable;
use crate::util::Result;
use failure::format_err;
use std::cell::RefCell;
use std::fmt;
use std::io::{Read, Write};
use std::path::Path;
use std::rc::Rc;

use axrom::Axrom;
use cnrom::Cnrom; // Mapper 3
                  // use exrom::Exrom;
use nrom::Nrom; // Mapper 0
use sxrom::Sxrom; // Mapper 1
use txrom::Txrom; // Mapper 4
use uxrom::Uxrom; // Mapper 2 // Mapper 5 // Mapper 7

pub mod axrom;
pub mod cnrom;
// pub mod exrom;
pub mod nrom;
pub mod sxrom;
pub mod txrom;
pub mod uxrom;

/// Alias for Mapper wrapped in a Rc/RefCell
pub type MapperRef = Rc<RefCell<Mapper>>;

/// Mapper trait requiring Memory + Send + Savable
pub trait Mapper: Memory + Savable + fmt::Debug {
    fn irq_pending(&mut self) -> bool;
    fn mirroring(&self) -> Mirroring;
    fn clock(&mut self, ppu: &Ppu);
    fn battery_backed(&self) -> bool;
    fn save_sram(&self, fh: &mut Write) -> Result<()>;
    fn load_sram(&mut self, fh: &mut Read) -> Result<()>;
    fn chr(&self) -> Option<&Banks<Ram>>;
    fn prg_rom(&self) -> Option<&Banks<Rom>>;
    fn prg_ram(&self) -> Option<&Ram>;
    fn reset(&mut self);
    fn power_cycle(&mut self);
}

pub fn null() -> MapperRef {
    NullMapper::load()
}

/// Attempts to return a valid Mapper for the given rom.
pub fn load_rom<P: AsRef<Path>>(rom: P) -> Result<MapperRef> {
    let cart = Cartridge::from_rom(rom)?;
    match cart.header.mapper_num {
        0 => Ok(Nrom::load(cart)),
        1 => Ok(Sxrom::load(cart)),
        2 => Ok(Uxrom::load(cart)),
        3 => Ok(Cnrom::load(cart)),
        4 => Ok(Txrom::load(cart)),
        // 5 => Ok(Exrom::load(cart)),
        7 => Ok(Axrom::load(cart)),
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
        (*self as u8).save(fh)
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

#[derive(Debug)]
pub struct NullMapper {}

impl NullMapper {
    pub fn load() -> MapperRef {
        Rc::new(RefCell::new(Self {}))
    }
}

impl Mapper for NullMapper {
    fn irq_pending(&mut self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        Mirroring::Horizontal
    }
    fn clock(&mut self, _ppu: &Ppu) {}
    fn battery_backed(&self) -> bool {
        false
    }
    fn save_sram(&self, _fh: &mut Write) -> Result<()> {
        Ok(())
    }
    fn load_sram(&mut self, _fh: &mut Read) -> Result<()> {
        Ok(())
    }
    fn chr(&self) -> Option<&Banks<Ram>> {
        None
    }
    fn prg_rom(&self) -> Option<&Banks<Rom>> {
        None
    }
    fn prg_ram(&self) -> Option<&Ram> {
        None
    }
    fn reset(&mut self) {}
    fn power_cycle(&mut self) {}
}

impl Memory for NullMapper {
    fn read(&mut self, _addr: u16) -> u8 {
        0
    }
    fn peek(&self, _addr: u16) -> u8 {
        0
    }
    fn write(&mut self, _addr: u16, _val: u8) {}
}

impl Savable for NullMapper {
    fn save(&self, _fh: &mut Write) -> Result<()> {
        Ok(())
    }
    fn load(&mut self, _fh: &mut Read) -> Result<()> {
        Ok(())
    }
}
