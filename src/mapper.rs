//! NES Memory Mappers for Cartridges
//!
//! [http://wiki.nesdev.com/w/index.php/Mapper]()

use crate::{
    cartridge::Cartridge,
    common::{Addr, Byte, Clocked, Powered},
    memory::{MemRead, MemWrite},
    serialization::Savable,
    {nes_err, NesResult},
};
use enum_dispatch::enum_dispatch;
use std::{
    fmt::Debug,
    io::{Read, Write},
};

use m000_nrom::Nrom; // Mapper 0
use m001_sxrom::Sxrom; // Mapper 1
use m002_uxrom::Uxrom; // Mapper 2
use m003_cnrom::Cnrom; // Mapper 3
use m004_txrom::Txrom; // Mapper 4
use m005_exrom::Exrom; // Mapper 5
use m007_axrom::Axrom; // Mapper 7
use m009_pxrom::Pxrom; // Mapper 9
use m155_mmc1a::Mapper155; // Mapper 155

mod m000_nrom;
mod m001_sxrom;
mod m002_uxrom;
mod m003_cnrom;
mod m004_txrom;
mod m005_exrom;
mod m007_axrom;
mod m009_pxrom;
mod m155_mmc1a;

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

#[derive(Debug, Copy, Clone)]
pub struct NullMapper {}

#[allow(clippy::large_enum_variant)]
#[enum_dispatch]
#[derive(Debug, Clone)]
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
    Mapper155,
}

#[enum_dispatch(MapperType)]
pub trait Mapper: MemRead + MemWrite + Savable + Clocked + Powered {
    fn irq_pending(&mut self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        Mirroring::Horizontal
    }
    fn vram_change(&mut self, _addr: Addr) {}
    fn battery_backed(&self) -> bool {
        false
    }
    fn save_sram<F: Write>(&self, _fh: &mut F) -> NesResult<()> {
        Ok(())
    }
    fn load_sram<F: Read>(&mut self, _fh: &mut F) -> NesResult<()> {
        Ok(())
    }
    fn use_ciram(&self, _addr: Addr) -> bool {
        true
    }
    fn nametable_page(&self, _addr: Addr) -> Addr {
        0
    }
    fn ppu_write(&mut self, _addr: Addr, _val: Byte) {}
    fn open_bus(&mut self, _addr: Addr, _val: Byte) {}
}

/// Attempts to return a valid Mapper for the given rom.
pub fn load_rom<F: Read>(name: &str, rom: &mut F, consistent_ram: bool) -> NesResult<MapperType> {
    let cart = Cartridge::from_rom(name, rom)?;
    let mapper = match cart.header.mapper_num {
        0 => Nrom::load(cart, consistent_ram),
        1 => Sxrom::load(cart, consistent_ram),
        2 => Uxrom::load(cart, consistent_ram),
        3 => Cnrom::load(cart, consistent_ram),
        4 => Txrom::load(cart, consistent_ram),
        5 => Exrom::load(cart, consistent_ram),
        7 => Axrom::load(cart, consistent_ram),
        9 => Pxrom::load(cart, consistent_ram),
        71 => Uxrom::load(cart, consistent_ram), // TODO: Mapper 71 has slight differences from Uxrom
        155 => Mapper155::load(cart, consistent_ram),
        _ => nes_err!("unsupported mapper number: {}", cart.header.mapper_num)?,
    };
    Ok(mapper)
}

impl Mapper for NullMapper {}
impl MemRead for NullMapper {}
impl MemWrite for NullMapper {}
impl Savable for NullMapper {}
impl Clocked for NullMapper {}
impl Powered for NullMapper {}

pub fn null() -> MapperType {
    let null = NullMapper {};
    null.into()
}

impl Savable for Mirroring {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
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
