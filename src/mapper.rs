//! NES Memory Mappers for Cartridges
//!
//! [http://wiki.nesdev.com/w/index.php/Mapper]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    logging::{LogLevel, Loggable},
    memory::{MemRead, MemWrite},
    serialization::Savable,
    {nes_err, NesResult},
};
use enum_dispatch::enum_dispatch;
use std::{
    cell::RefCell,
    fmt,
    io::{Read, Write},
    rc::Rc,
};

use m000_nrom::Nrom; // Mapper 0
use m001_sxrom::Sxrom; // Mapper 1
use m002_uxrom::Uxrom; // Mapper 2
use m003_cnrom::Cnrom; // Mapper 3
use m004_txrom::Txrom; // Mapper 4
use m005_exrom::Exrom; // Mapper 5
use m007_axrom::Axrom; // Mapper 7
use m009_pxrom::Pxrom; // Mapper 9

mod m000_nrom;
mod m001_sxrom;
mod m002_uxrom;
mod m003_cnrom;
mod m004_txrom;
mod m005_exrom;
mod m007_axrom;
mod m009_pxrom;

/// Alias for Mapper wrapped in a Rc/RefCell
pub type MapperRef = Rc<RefCell<MapperType>>;

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

#[derive(Debug)]
pub struct NullMapper {}

#[allow(clippy::large_enum_variant)]
#[enum_dispatch]
#[derive(Debug)]
pub enum MapperType {
    NullMapper,
    Nrom,
    Sxrom,
    Uxrom,
    Cnrom,
    Txrom,
    Exrom,
    Axrom,
    Pxrom,
}

/// Mapper trait requiring Memory + Send + Savable
#[enum_dispatch(MapperType)]
pub trait Mapper: MemRead + MemWrite + Savable + Clocked + Powered + Loggable + fmt::Debug {
    fn irq_pending(&mut self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        Mirroring::Horizontal
    }
    fn vram_change(&mut self, _addr: u16) {}
    fn battery_backed(&self) -> bool {
        false
    }
    fn save_sram(&self, _fh: &mut dyn Write) -> NesResult<()> {
        Ok(())
    }
    fn load_sram(&mut self, _fh: &mut dyn Read) -> NesResult<()> {
        Ok(())
    }
    fn use_ciram(&self, _addr: u16) -> bool {
        true
    }
    fn nametable_page(&self, _addr: u16) -> u16 {
        0
    }
    fn ppu_write(&mut self, _addr: u16, _val: u8) {}
    fn open_bus(&mut self, _addr: u16, _val: u8) {}
}

/// Attempts to return a valid Mapper for the given rom.
pub fn load_rom(rom: &str) -> NesResult<MapperRef> {
    let cart = Cartridge::from_rom(rom)?;
    let mapper = match cart.header.mapper_num {
        0 => Nrom::load(cart),
        1 => Sxrom::load(cart),
        2 => Uxrom::load(cart),
        3 => Cnrom::load(cart),
        4 => Txrom::load(cart),
        5 => Exrom::load(cart),
        7 => Axrom::load(cart),
        9 => Pxrom::load(cart),
        // 71 => Uxrom::load(cart), // TODO - Variant of Uxrom with submappers
        _ => nes_err!("unsupported mapper number: {}", cart.header.mapper_num)?,
    };
    Ok(Rc::new(RefCell::new(mapper)))
}

impl Mapper for NullMapper {}
impl MemRead for NullMapper {}
impl MemWrite for NullMapper {}
impl Savable for NullMapper {}
impl Clocked for NullMapper {}
impl Powered for NullMapper {}
impl Loggable for NullMapper {}

pub fn null() -> MapperRef {
    let null = NullMapper {};
    Rc::new(RefCell::new(null.into()))
}

impl Savable for Mirroring {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
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

impl Default for Mirroring {
    fn default() -> Self {
        Mirroring::Horizontal
    }
}
