//! PPU Memory/Data Bus.

use crate::{
    common::{NesRegion, Regional, Reset, ResetKind},
    mapper::{BusKind, MapRead, MapWrite, MappedRead, MappedWrite, Mapper, Mirrored, OnBusWrite},
    mem::{ConstArray, Memory, RamState, Read, Write},
    ppu::{Mirroring, Ppu},
};
use serde::{Deserialize, Serialize};
use tracing::error;

pub trait PpuAddr {
    /// Returns whether this value can be used to fetch a nametable attribute byte.
    fn is_attr(&self) -> bool;
    fn is_palette(&self) -> bool;
}

impl PpuAddr for u16 {
    fn is_attr(&self) -> bool {
        (*self & (Ppu::NT_SIZE - 1)) >= Ppu::ATTR_OFFSET
    }

    fn is_palette(&self) -> bool {
        *self >= Ppu::PALETTE_START
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bus {
    pub mapper: Mapper,
    pub chr: Memory<Box<[u8]>>,
    pub ciram: Memory<ConstArray<u8, 0x0800>>, // $2007 PPUDATA
    pub palette: Memory<ConstArray<u8, 32>>,
    #[serde(skip)]
    pub exram: Memory<Box<[u8]>>,
    pub open_bus: u8,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(RamState::default())
    }
}

impl Bus {
    pub const VRAM_SIZE: usize = 0x0800; // Two 1k Nametables
    pub const PALETTE_SIZE: usize = 32; // 32 possible colors at a time

    pub fn new(ram_state: RamState) -> Self {
        Self {
            mapper: Mapper::none(),
            chr: Memory::rom(0),
            ciram: Memory::ram_const(ram_state),
            palette: Memory::ram_const(ram_state),
            exram: Memory::empty(),
            open_bus: 0x00,
        }
    }

    pub fn mirroring(&self) -> Mirroring {
        self.mapper.mirroring()
    }

    pub fn load_chr(&mut self, chr: Memory<Box<[u8]>>) {
        self.chr = chr;
    }

    pub fn load_ex_ram(&mut self, ex_ram: Memory<Box<[u8]>>) {
        self.exram = ex_ram;
    }

    // Maps addresses to nametable pages based on mirroring mode
    //
    // Vram:            [ A ] [ B ]
    //
    // Horizontal:      [ A ] [ a ]
    //                  [ B ] [ b ]
    //
    // Vertical:        [ A ] [ B ]
    //                  [ a ] [ b ]
    //
    // Single Screen A: [ A ] [ a ]
    //                  [ a ] [ a ]
    //
    // Single Screen B: [ b ] [ B ]
    //                  [ b ] [ b ]
    //
    // Fourscreen should not use this method and instead should rely on mapper translation.

    pub const fn ciram_mirror(addr: u16, mirroring: Mirroring) -> usize {
        let nametable = (addr >> mirroring as u16) & Ppu::NT_SIZE;
        (nametable | (!nametable & addr & 0x03FF)) as usize
    }

    const fn palette_mirror(&self, addr: u16) -> usize {
        let addr = addr & 0x001F;
        let addr = if addr >= 16 && addr.trailing_zeros() >= 2 {
            addr - 16
        } else {
            addr
        };
        addr as usize
    }

    pub fn read_ciram(&mut self, addr: u16) -> u8 {
        match self.mapper.map_read(addr) {
            MappedRead::Bus => self.ciram[Self::ciram_mirror(addr, self.mirroring())],
            MappedRead::CIRam(addr) => self.ciram[addr],
            MappedRead::ExRam(addr) => self.exram[addr],
            MappedRead::Data(data) => data,
            MappedRead::Chr(addr) => self.chr[addr],
            MappedRead::PrgRom(mapped) => {
                tracing::warn!("unexpected mapped PRG-ROM read at ${addr:04X} ${mapped:04X}");
                0
            }
            MappedRead::PrgRam(mapped) => {
                tracing::warn!("unexpected mapped PRG-RAM read at ${addr:04X} ${mapped:04X}");
                0
            }
        }
    }

    pub fn peek_ciram(&self, addr: u16) -> u8 {
        match self.mapper.map_peek(addr) {
            MappedRead::Bus => self.ciram[Self::ciram_mirror(addr, self.mirroring())],
            MappedRead::CIRam(addr) => self.ciram[addr],
            MappedRead::ExRam(addr) => self.exram[addr],
            MappedRead::Data(data) => data,
            MappedRead::Chr(addr) => self.chr[addr],
            MappedRead::PrgRom(mapped) => {
                tracing::warn!("unexpected mapped PRG-ROM read at ${addr:04X} ${mapped:04X}");
                0
            }
            MappedRead::PrgRam(mapped) => {
                tracing::warn!("unexpected mapped PRG-RAM read at ${addr:04X} ${mapped:04X}");
                0
            }
        }
    }

    pub fn read_chr(&mut self, addr: u16) -> u8 {
        let addr = if let MappedRead::Chr(addr) = self.mapper.map_read(addr) {
            addr
        } else {
            addr.into()
        };
        self.chr[addr]
    }

    pub fn peek_chr(&self, addr: u16) -> u8 {
        let addr = if let MappedRead::Chr(addr) = self.mapper.map_peek(addr) {
            addr
        } else {
            addr.into()
        };
        self.chr[addr]
    }

    #[inline]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn peek_palette(&self, addr: u16) -> u8 {
        self.palette[self.palette_mirror(addr)]
    }
}

impl Read for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        let val = match addr {
            0x2000..=0x3EFF => self.read_ciram(addr),
            0x0000..=0x1FFF => self.read_chr(addr),
            0x3F00..=0x3FFF => self.peek_palette(addr),
            _ => {
                error!("unexpected PPU memory access at ${:04X}", addr);
                0x00
            }
        };
        self.open_bus = val;
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x2000..=0x3EFF => self.peek_ciram(addr),
            0x0000..=0x1FFF => self.peek_chr(addr),
            0x3F00..=0x3FFF => self.peek_palette(addr),
            _ => {
                error!("unexpected PPU memory access at ${:04X}", addr);
                0x00
            }
        }
    }
}

impl Write for Bus {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x3EFF => match self.mapper.map_write(addr, val) {
                MappedWrite::Bus => {
                    let addr = Self::ciram_mirror(addr, self.mirroring());
                    self.ciram[addr] = val;
                }
                MappedWrite::CIRam(addr, val) => self.ciram[addr] = val,
                MappedWrite::ExRam(addr, val) => self.exram[addr] = val,
                MappedWrite::ChrRam(addr, val) => {
                    if self.chr.is_ram() {
                        self.chr[addr] = val;
                    }
                }
                MappedWrite::PrgRam(mapped, val) => {
                    tracing::warn!(
                        "unexpected mapped PRG-RAM write at ${addr:04X} for ${mapped:04X} with ${val:02X}"
                    );
                }
                MappedWrite::PrgRamProtect(val) => {
                    tracing::warn!(
                        "unexpected mapped PRG-RAM Protect write at ${addr:04X} with {val}"
                    );
                }
                MappedWrite::None => (),
            },
            0x3F00..=0x3FFF => {
                let addr = self.palette_mirror(addr);
                self.palette[addr] = val;
            }
            _ => error!("unexpected PPU memory access at ${:04X}", addr),
        }
        self.mapper.on_bus_write(addr, val, BusKind::Ppu);
        self.open_bus = val;
    }
}

impl Regional for Bus {
    fn region(&self) -> NesRegion {
        self.mapper.region()
    }

    fn set_region(&mut self, region: NesRegion) {
        self.mapper.set_region(region);
    }
}

impl Reset for Bus {
    fn reset(&mut self, kind: ResetKind) {
        self.open_bus = 0x00;
        if self.chr.is_ram() {
            self.chr.reset(kind);
        }
        self.mapper.reset(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ciram_mirror_horizontal() {
        assert_eq!(Bus::ciram_mirror(0x2000, Mirroring::Horizontal), 0x0000);
        assert_eq!(Bus::ciram_mirror(0x2005, Mirroring::Horizontal), 0x0005);
        assert_eq!(Bus::ciram_mirror(0x23FF, Mirroring::Horizontal), 0x03FF);
        assert_eq!(Bus::ciram_mirror(0x2400, Mirroring::Horizontal), 0x0000);
        assert_eq!(Bus::ciram_mirror(0x2405, Mirroring::Horizontal), 0x0005);
        assert_eq!(Bus::ciram_mirror(0x27FF, Mirroring::Horizontal), 0x03FF);
        assert_eq!(Bus::ciram_mirror(0x2800, Mirroring::Horizontal), 0x0400);
        assert_eq!(Bus::ciram_mirror(0x2805, Mirroring::Horizontal), 0x0405);
        assert_eq!(Bus::ciram_mirror(0x2BFF, Mirroring::Horizontal), 0x07FF);
        assert_eq!(Bus::ciram_mirror(0x2C00, Mirroring::Horizontal), 0x0400);
        assert_eq!(Bus::ciram_mirror(0x2C05, Mirroring::Horizontal), 0x0405);
        assert_eq!(Bus::ciram_mirror(0x2FFF, Mirroring::Horizontal), 0x07FF);
    }

    #[test]
    fn ciram_mirror_vertical() {
        assert_eq!(Bus::ciram_mirror(0x2000, Mirroring::Vertical), 0x0000);
        assert_eq!(Bus::ciram_mirror(0x2005, Mirroring::Vertical), 0x0005);
        assert_eq!(Bus::ciram_mirror(0x23FF, Mirroring::Vertical), 0x03FF);
        assert_eq!(Bus::ciram_mirror(0x2800, Mirroring::Vertical), 0x0000);
        assert_eq!(Bus::ciram_mirror(0x2805, Mirroring::Vertical), 0x0005);
        assert_eq!(Bus::ciram_mirror(0x2BFF, Mirroring::Vertical), 0x03FF);
        assert_eq!(Bus::ciram_mirror(0x2400, Mirroring::Vertical), 0x0400);
        assert_eq!(Bus::ciram_mirror(0x2405, Mirroring::Vertical), 0x0405);
        assert_eq!(Bus::ciram_mirror(0x27FF, Mirroring::Vertical), 0x07FF);
        assert_eq!(Bus::ciram_mirror(0x2C00, Mirroring::Vertical), 0x0400);
        assert_eq!(Bus::ciram_mirror(0x2C05, Mirroring::Vertical), 0x0405);
        assert_eq!(Bus::ciram_mirror(0x2FFF, Mirroring::Vertical), 0x07FF);
    }

    #[test]
    fn ciram_mirror_single_screen_a() {
        assert_eq!(Bus::ciram_mirror(0x2000, Mirroring::SingleScreenA), 0x0000);
        assert_eq!(Bus::ciram_mirror(0x2005, Mirroring::SingleScreenA), 0x0005);
        assert_eq!(Bus::ciram_mirror(0x23FF, Mirroring::SingleScreenA), 0x03FF);
        assert_eq!(Bus::ciram_mirror(0x2800, Mirroring::SingleScreenA), 0x0000);
        assert_eq!(Bus::ciram_mirror(0x2805, Mirroring::SingleScreenA), 0x0005);
        assert_eq!(Bus::ciram_mirror(0x2BFF, Mirroring::SingleScreenA), 0x03FF);
        assert_eq!(Bus::ciram_mirror(0x2400, Mirroring::SingleScreenA), 0x0000);
        assert_eq!(Bus::ciram_mirror(0x2405, Mirroring::SingleScreenA), 0x0005);
        assert_eq!(Bus::ciram_mirror(0x27FF, Mirroring::SingleScreenA), 0x03FF);
        assert_eq!(Bus::ciram_mirror(0x2C00, Mirroring::SingleScreenA), 0x0000);
        assert_eq!(Bus::ciram_mirror(0x2C05, Mirroring::SingleScreenA), 0x0005);
        assert_eq!(Bus::ciram_mirror(0x2FFF, Mirroring::SingleScreenA), 0x03FF);
    }

    #[test]
    fn ciram_mirror_single_screen_b() {
        assert_eq!(Bus::ciram_mirror(0x2000, Mirroring::SingleScreenB), 0x0400);
        assert_eq!(Bus::ciram_mirror(0x2005, Mirroring::SingleScreenB), 0x0405);
        assert_eq!(Bus::ciram_mirror(0x23FF, Mirroring::SingleScreenB), 0x07FF);
        assert_eq!(Bus::ciram_mirror(0x2800, Mirroring::SingleScreenB), 0x0400);
        assert_eq!(Bus::ciram_mirror(0x2805, Mirroring::SingleScreenB), 0x0405);
        assert_eq!(Bus::ciram_mirror(0x2BFF, Mirroring::SingleScreenB), 0x07FF);
        assert_eq!(Bus::ciram_mirror(0x2400, Mirroring::SingleScreenB), 0x0400);
        assert_eq!(Bus::ciram_mirror(0x2405, Mirroring::SingleScreenB), 0x0405);
        assert_eq!(Bus::ciram_mirror(0x27FF, Mirroring::SingleScreenB), 0x07FF);
        assert_eq!(Bus::ciram_mirror(0x2C00, Mirroring::SingleScreenB), 0x0400);
        assert_eq!(Bus::ciram_mirror(0x2C05, Mirroring::SingleScreenB), 0x0405);
        assert_eq!(Bus::ciram_mirror(0x2FFF, Mirroring::SingleScreenB), 0x07FF);
    }
}
