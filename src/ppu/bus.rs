use crate::{
    common::{Kind, NesRegion, Regional, Reset},
    mapper::{Mapped, MappedRead, MappedWrite, Mapper, MemMap},
    mem::{Access, Mem},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

pub trait PpuAddr {
    /// Returns whether this value can be used to fetch a nametable attribute byte.
    fn is_attr(&self) -> bool;
    fn is_palette(&self) -> bool;
}

impl PpuAddr for u16 {
    #[inline]
    fn is_attr(&self) -> bool {
        (*self & 0x03FF) >= 0x03C0
    }
    #[inline]
    fn is_palette(&self) -> bool {
        *self >= 0x3F00
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct PpuBus {
    mapper: Mapper,
    mirror_shift: usize,
    ciram: Vec<u8>, // $2007 PPUDATA
    palette: [u8; Self::PALETTE_SIZE],
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    exram: Vec<u8>,
    open_bus: u8,
}

impl Default for PpuBus {
    fn default() -> Self {
        Self::new()
    }
}

impl PpuBus {
    const VRAM_SIZE: usize = 0x0800; // Two 1k Nametables
    const PALETTE_SIZE: usize = 32; // 32 possible colors at a time

    pub fn new() -> Self {
        Self {
            mapper: Mapper::none(),
            mirror_shift: Mirroring::default() as usize,
            ciram: vec![0x00; Self::VRAM_SIZE],
            palette: [0x00; Self::PALETTE_SIZE],
            chr_rom: vec![],
            chr_ram: vec![],
            exram: vec![],
            open_bus: 0x00,
        }
    }

    #[inline]
    pub fn mirroring(&self) -> Mirroring {
        self.mapper.mirroring()
    }

    #[inline]
    pub fn update_mirroring(&mut self) {
        self.mirror_shift = self.mirroring() as usize;
    }

    #[inline]
    pub fn load_chr_rom(&mut self, chr_rom: Vec<u8>) {
        self.chr_rom = chr_rom;
    }

    #[inline]
    pub fn load_chr_ram(&mut self, chr_ram: Vec<u8>) {
        self.chr_ram = chr_ram;
    }

    #[inline]
    pub fn load_ex_ram(&mut self, ex_ram: Vec<u8>) {
        self.exram = ex_ram;
    }

    #[inline]
    pub fn load_mapper(&mut self, mapper: Mapper) {
        self.mapper = mapper;
    }

    #[inline]
    pub const fn mapper(&self) -> &Mapper {
        &self.mapper
    }

    #[inline]
    pub fn mapper_mut(&mut self) -> &mut Mapper {
        &mut self.mapper
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
    #[inline]
    const fn ciram_mirror(&self, addr: usize) -> usize {
        let nametable = (addr >> self.mirror_shift) & 0x0400;
        (nametable) | (!nametable & addr & 0x03FF)
    }

    #[inline]
    const fn palette_mirror(&self, addr: usize) -> usize {
        let addr = addr & 0x001F;
        if addr >= 16 && addr.trailing_zeros() >= 2 {
            addr - 16
        } else {
            addr
        }
    }
}

impl Mem for PpuBus {
    fn read(&mut self, addr: u16, _access: Access) -> u8 {
        let val = match addr {
            0x0000..=0x1FFF => {
                let addr = if let MappedRead::Chr(addr) = self.mapper.map_read(addr) {
                    addr
                } else {
                    addr.into()
                };
                if self.chr_rom.is_empty() {
                    self.chr_ram[addr]
                } else {
                    self.chr_rom[addr]
                }
            }
            0x2000..=0x3EFF => match self.mapper.map_read(addr) {
                MappedRead::CIRam(addr) => self.ciram[addr],
                MappedRead::ExRam(addr) => self.exram[addr],
                MappedRead::Data(data) => data,
                _ => {
                    if self.mirroring() == Mirroring::FourScreen {
                        0x00
                    } else {
                        self.ciram[self.ciram_mirror(addr as usize)]
                    }
                }
            },
            0x3F00..=0x3FFF => self.palette[self.palette_mirror(addr as usize)],
            _ => {
                log::error!("unexpected PPU memory access at ${:04X}", addr);
                0x00
            }
        };
        self.open_bus = val;
        val
    }

    fn peek(&self, addr: u16, _access: Access) -> u8 {
        match addr {
            0x2000..=0x3EFF => match self.mapper.map_peek(addr) {
                MappedRead::CIRam(addr) => self.ciram[addr],
                MappedRead::ExRam(addr) => self.exram[addr],
                MappedRead::Data(data) => data,
                _ => {
                    if self.mirroring() == Mirroring::FourScreen {
                        0x00
                    } else {
                        self.ciram[self.ciram_mirror(addr as usize)]
                    }
                }
            },
            0x0000..=0x1FFF => {
                let addr = if let MappedRead::Chr(addr) = self.mapper.map_peek(addr) {
                    addr
                } else {
                    addr.into()
                };
                if !self.chr_ram.is_empty() {
                    self.chr_ram[addr]
                } else {
                    self.chr_rom[addr]
                }
            }
            0x3F00..=0x3FFF => self.palette[self.palette_mirror(addr as usize)],
            _ => {
                log::error!("unexpected PPU memory access at ${:04X}", addr);
                0x00
            }
        }
    }

    fn write(&mut self, addr: u16, val: u8, _access: Access) {
        match addr {
            0x2000..=0x3EFF => match self.mapper.map_write(addr, val) {
                MappedWrite::CIRam(addr, val) => self.ciram[addr] = val,
                MappedWrite::ExRam(addr, val) => self.exram[addr] = val,
                _ => {
                    if self.mirroring() != Mirroring::FourScreen {
                        let addr = self.ciram_mirror(addr as usize);
                        self.ciram[addr] = val;
                    }
                }
            },
            0x0000..=0x1FFF => {
                if !self.chr_ram.is_empty() {
                    if let MappedWrite::Chr(addr, val) = self.mapper.map_write(addr, val) {
                        self.chr_ram[addr] = val;
                    }
                }
            }
            0x3F00..=0x3FFF => {
                self.palette[self.palette_mirror(addr as usize)] = val;
            }
            _ => log::error!("unexpected PPU memory access at ${:04X}", addr),
        }
        self.mapper.ppu_bus_write(addr, val);
        self.open_bus = val;
    }
}

impl Regional for PpuBus {
    #[inline]
    fn region(&self) -> NesRegion {
        self.mapper.region()
    }

    fn set_region(&mut self, region: NesRegion) {
        self.mapper.set_region(region);
    }
}

impl Reset for PpuBus {
    fn reset(&mut self, kind: Kind) {
        self.open_bus = 0x00;
        self.mapper.reset(kind);
    }
}

impl std::fmt::Debug for PpuBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PpuBus")
            .field("mapper", &self.mapper)
            .field("ciram_len", &self.ciram.len())
            .field("palette_len", &self.palette.len())
            .field("chr_rom_len", &self.chr_rom.len())
            .field("chr_ram_len", &self.chr_ram.len())
            .field("ex_ram_len", &self.exram.len())
            .field("open_bus", &self.open_bus)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ciram_mirror_horizontal() {
        let mut ppu_bus = PpuBus::new();
        ppu_bus.mirror_shift = Mirroring::Horizontal as usize;

        assert_eq!(ppu_bus.ciram_mirror(0x2000), 0x0000);
        assert_eq!(ppu_bus.ciram_mirror(0x2005), 0x0005);
        assert_eq!(ppu_bus.ciram_mirror(0x23FF), 0x03FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2400), 0x0000);
        assert_eq!(ppu_bus.ciram_mirror(0x2405), 0x0005);
        assert_eq!(ppu_bus.ciram_mirror(0x27FF), 0x03FF);

        assert_eq!(ppu_bus.ciram_mirror(0x2800), 0x0400);
        assert_eq!(ppu_bus.ciram_mirror(0x2805), 0x0405);
        assert_eq!(ppu_bus.ciram_mirror(0x2BFF), 0x07FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2C00), 0x0400);
        assert_eq!(ppu_bus.ciram_mirror(0x2C05), 0x0405);
        assert_eq!(ppu_bus.ciram_mirror(0x2FFF), 0x07FF);
    }

    #[test]
    fn ciram_mirror_vertical() {
        let mut ppu_bus = PpuBus::new();
        ppu_bus.mirror_shift = Mirroring::Vertical as usize;

        assert_eq!(ppu_bus.ciram_mirror(0x2000), 0x0000);
        assert_eq!(ppu_bus.ciram_mirror(0x2005), 0x0005);
        assert_eq!(ppu_bus.ciram_mirror(0x23FF), 0x03FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2800), 0x0000);
        assert_eq!(ppu_bus.ciram_mirror(0x2805), 0x0005);
        assert_eq!(ppu_bus.ciram_mirror(0x2BFF), 0x03FF);

        assert_eq!(ppu_bus.ciram_mirror(0x2400), 0x0400);
        assert_eq!(ppu_bus.ciram_mirror(0x2405), 0x0405);
        assert_eq!(ppu_bus.ciram_mirror(0x27FF), 0x07FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2C00), 0x0400);
        assert_eq!(ppu_bus.ciram_mirror(0x2C05), 0x0405);
        assert_eq!(ppu_bus.ciram_mirror(0x2FFF), 0x07FF);
    }

    #[test]
    fn ciram_mirror_single_screen_a() {
        let mut ppu_bus = PpuBus::new();
        ppu_bus.mirror_shift = Mirroring::SingleScreenA as usize;

        assert_eq!(ppu_bus.ciram_mirror(0x2000), 0x0000);
        assert_eq!(ppu_bus.ciram_mirror(0x2005), 0x0005);
        assert_eq!(ppu_bus.ciram_mirror(0x23FF), 0x03FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2800), 0x0000);
        assert_eq!(ppu_bus.ciram_mirror(0x2805), 0x0005);
        assert_eq!(ppu_bus.ciram_mirror(0x2BFF), 0x03FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2400), 0x0000);
        assert_eq!(ppu_bus.ciram_mirror(0x2405), 0x0005);
        assert_eq!(ppu_bus.ciram_mirror(0x27FF), 0x03FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2C00), 0x0000);
        assert_eq!(ppu_bus.ciram_mirror(0x2C05), 0x0005);
        assert_eq!(ppu_bus.ciram_mirror(0x2FFF), 0x03FF);
    }

    #[test]
    fn ciram_mirror_single_screen_b() {
        let mut ppu_bus = PpuBus::new();
        ppu_bus.mirror_shift = Mirroring::SingleScreenB as usize;

        assert_eq!(ppu_bus.ciram_mirror(0x2000), 0x0400);
        assert_eq!(ppu_bus.ciram_mirror(0x2005), 0x0405);
        assert_eq!(ppu_bus.ciram_mirror(0x23FF), 0x07FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2800), 0x0400);
        assert_eq!(ppu_bus.ciram_mirror(0x2805), 0x0405);
        assert_eq!(ppu_bus.ciram_mirror(0x2BFF), 0x07FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2400), 0x0400);
        assert_eq!(ppu_bus.ciram_mirror(0x2405), 0x0405);
        assert_eq!(ppu_bus.ciram_mirror(0x27FF), 0x07FF);
        assert_eq!(ppu_bus.ciram_mirror(0x2C00), 0x0400);
        assert_eq!(ppu_bus.ciram_mirror(0x2C05), 0x0405);
        assert_eq!(ppu_bus.ciram_mirror(0x2FFF), 0x07FF);
    }
}
