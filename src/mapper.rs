//! NES Memory Mappers for Cartridges
//!
//! [http://wiki.nesdev.com/w/index.php/Mapper]()

use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::memory::{Banks, Memory, Ram, Rom};
use crate::serialization::Savable;
use crate::{nes_err, Result};
use std::cell::RefCell;
use std::fmt;
use std::io::{Read, Write};
use std::path::Path;
use std::rc::Rc;

use axrom::Axrom; // Mapper 7
use cnrom::Cnrom; // Mapper 3
use exrom::Exrom; // Mapper 5
use nrom::Nrom; // Mapper 0
use pxrom::Pxrom; // Mapper 9
use sxrom::Sxrom; // Mapper 1
use txrom::Txrom; // Mapper 4
use uxrom::Uxrom; // Mapper 2

pub mod axrom;
pub mod cnrom;
pub mod exrom;
pub mod nrom;
pub mod pxrom;
pub mod sxrom;
pub mod txrom;
pub mod uxrom;

/// Alias for Mapper wrapped in a Rc/RefCell
pub type MapperRef = Rc<RefCell<dyn Mapper>>;

/// Mapper trait requiring Memory + Send + Savable
pub trait Mapper: Memory + Savable + fmt::Debug {
    fn irq_pending(&mut self) -> bool;
    fn mirroring(&self) -> Mirroring;
    fn vram_change(&mut self, addr: u16);
    fn clock(&mut self, ppu: &Ppu);
    fn battery_backed(&self) -> bool;
    fn save_sram(&self, fh: &mut dyn Write) -> Result<()>;
    fn load_sram(&mut self, fh: &mut dyn Read) -> Result<()>;
    fn chr(&self) -> Option<&Banks<Ram>>;
    fn prg_rom(&self) -> Option<&Banks<Rom>>;
    fn prg_ram(&self) -> Option<&Ram>;
    fn logging(&mut self, logging: bool);
    fn use_ciram(&self, addr: u16) -> bool;
    fn nametable_addr(&self, addr: u16) -> u16;
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
        5 => Ok(Exrom::load(cart)),
        7 => Ok(Axrom::load(cart)),
        9 => Ok(Pxrom::load(cart)),
        _ => Err(nes_err!(
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
    SingleScreenA,
    SingleScreenB,
    FourScreen,
}

impl Savable for Mirroring {
    fn save(&self, fh: &mut dyn Write) -> Result<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> Result<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => Mirroring::Horizontal,
            1 => Mirroring::Vertical,
            2 => Mirroring::SingleScreenA,
            3 => Mirroring::SingleScreenB,
            4 => Mirroring::FourScreen,
            _ => panic!("invalid Mirroring value {}", val),
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
    fn vram_change(&mut self, _addr: u16) {}
    fn clock(&mut self, _ppu: &Ppu) {}
    fn battery_backed(&self) -> bool {
        false
    }
    fn save_sram(&self, _fh: &mut dyn Write) -> Result<()> {
        Ok(())
    }
    fn load_sram(&mut self, _fh: &mut dyn Read) -> Result<()> {
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
    fn logging(&mut self, _logging: bool) {}
    fn use_ciram(&self, _addr: u16) -> bool {
        true
    }
    fn nametable_addr(&self, _addr: u16) -> u16 {
        0
    }
}

impl Memory for NullMapper {
    fn read(&mut self, _addr: u16) -> u8 {
        0
    }
    fn peek(&self, _addr: u16) -> u8 {
        0
    }
    fn write(&mut self, _addr: u16, _val: u8) {}
    fn reset(&mut self) {}
    fn power_cycle(&mut self) {}
}

impl Savable for NullMapper {
    fn save(&self, _fh: &mut dyn Write) -> Result<()> {
        Ok(())
    }
    fn load(&mut self, _fh: &mut dyn Read) -> Result<()> {
        Ok(())
    }
}
