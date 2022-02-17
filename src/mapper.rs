//! NES Memory Mappers for Cartridges
//!
//! <http://wiki.nesdev.com/w/index.php/Mapper>

use crate::{
    cartridge::Cartridge,
    common::{Addr, Byte, Clocked, Powered},
    memory::{MemRead, MemWrite, RamState},
    serialization::Savable,
    NesResult,
};
use anyhow::anyhow;
use enum_dispatch::enum_dispatch;
use std::{
    fmt::Debug,
    io::{Read, Write},
};

use m000_nrom::Nrom;
use m001_sxrom::Sxrom;
use m002_uxrom::Uxrom;
use m003_cnrom::Cnrom;
use m004_txrom::Txrom;
use m005_exrom::Exrom;
use m007_axrom::Axrom;
use m009_pxrom::Pxrom;
use m071_bf909x::Bf909x;
use m155_mmc1a::Mmc1a;

mod m000_nrom;
mod m001_sxrom;
mod m002_uxrom;
mod m003_cnrom;
mod m004_txrom;
mod m005_exrom;
mod m007_axrom;
mod m009_pxrom;
mod m071_bf909x;
mod m155_mmc1a;

/// Nametable Mirroring Mode
///
/// <http://wiki.nesdev.com/w/index.php/Mirroring#Nametable_Mirroring>
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[must_use]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleScreenA,
    SingleScreenB,
    FourScreen,
}

#[derive(Debug, Copy, Clone)]
#[must_use]
pub struct NullMapper {}

#[allow(clippy::large_enum_variant)]
#[enum_dispatch]
#[derive(Debug, Clone)]
#[must_use]
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
    Bf909x,
    Mmc1a,
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
    /// Save SRAM data to filehnadle.
    ///
    /// # Errors
    ///
    /// If save fails, an error is returned.
    fn save_sram<F: Write>(&self, _fh: &mut F) -> NesResult<()> {
        Ok(())
    }
    /// Load SRAM data from filehnadle.
    ///
    /// # Errors
    ///
    /// If load fails, an error is returned.
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
///
/// # Errors
///
/// If loaded ROM has invalid headers or data, an error is returned.
pub fn load_rom<F: Read>(name: &str, rom: &mut F, state: RamState) -> NesResult<MapperType> {
    let cart = Cartridge::from_rom(name, rom)?;
    let mapper = match cart.header.mapper_num {
        0 => Nrom::load(cart, state),
        1 => Sxrom::load(cart, state),
        2 => Uxrom::load(cart, state),
        3 => Cnrom::load(cart),
        4 => Txrom::load(cart, state),
        5 => Exrom::load(cart, state),
        7 => Axrom::load(cart, state),
        9 => Pxrom::load(cart, state),
        71 => Bf909x::load(cart, state),
        155 => Mmc1a::load(cart, state),
        _ => {
            return Err(anyhow!(
                "unsupported mapper number: {}",
                cart.header.mapper_num
            ))
        }
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
