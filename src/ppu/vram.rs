use super::{
    nametable::{Nametable, NT_SIZE, NT_START},
    palette::{Palette, PALETTE_SIZE},
};
use crate::{
    common::Powered,
    mapper::{self, MapperRef, Mirroring},
    memory::{MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

#[derive(Clone)]
pub(super) struct Vram {
    nametable: Nametable, // Used to layout backgrounds on the screen
    palette: Palette,     // Background/Sprite color palettes
    pub(super) mapper: MapperRef,
    pub(super) buffer: u8, // PPUDATA buffer
}

impl Vram {
    pub(super) fn new() -> Self {
        Self {
            nametable: Nametable::new([0u8; NT_SIZE]),
            palette: Palette::new([0u8; PALETTE_SIZE]),
            mapper: mapper::null(),
            buffer: 0u8,
        }
    }

    pub(super) fn nametable_addr(&self, addr: u16) -> u16 {
        let mirroring = self.mapper.borrow().mirroring();
        // Maps addresses to nametable pages based on mirroring mode
        let page = match mirroring {
            Mirroring::Horizontal => (addr >> 11) & 1,
            Mirroring::Vertical => (addr >> 10) & 1,
            Mirroring::SingleScreenA => (addr >> 14) & 1,
            Mirroring::SingleScreenB => (addr >> 13) & 1,
            Mirroring::FourScreen => self.mapper.borrow().nametable_page(addr),
        };
        let table_size = 0x0400;
        let offset = addr % table_size;
        NT_START + page * table_size + offset
    }
}

impl MemRead for Vram {
    fn read(&mut self, addr: u16) -> u8 {
        self.mapper.borrow_mut().vram_change(addr);
        match addr {
            0x0000..=0x1FFF => self.mapper.borrow_mut().read(addr),
            0x2000..=0x3EFF => {
                // Use PPU Nametables or Cartridge RAM
                if self.mapper.borrow().use_ciram(addr) {
                    let mirror_addr = self.nametable_addr(addr);
                    self.nametable.read(mirror_addr % NT_SIZE as u16)
                } else {
                    self.mapper.borrow_mut().read(addr)
                }
            }
            0x3F00..=0x3FFF => self.palette.read(addr % PALETTE_SIZE as u16),
            _ => 0,
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.mapper.borrow().peek(addr),
            0x2000..=0x3EFF => {
                // Use PPU Nametables or Cartridge RAM
                if self.mapper.borrow().use_ciram(addr) {
                    let mirror_addr = self.nametable_addr(addr);
                    self.nametable.peek(mirror_addr % NT_SIZE as u16)
                } else {
                    self.mapper.borrow().peek(addr)
                }
            }
            0x3F00..=0x3FFF => self.palette.peek(addr % PALETTE_SIZE as u16),
            _ => 0,
        }
    }
}
impl MemWrite for Vram {
    fn write(&mut self, addr: u16, val: u8) {
        self.mapper.borrow_mut().vram_change(addr);
        match addr {
            0x0000..=0x1FFF => self.mapper.borrow_mut().write(addr, val),
            0x2000..=0x3EFF => {
                if self.mapper.borrow().use_ciram(addr) {
                    let mirror_addr = self.nametable_addr(addr);
                    self.nametable.write(mirror_addr % NT_SIZE as u16, val);
                } else {
                    self.mapper.borrow_mut().write(addr, val);
                }
            }
            0x3F00..=0x3FFF => self.palette.write(addr % PALETTE_SIZE as u16, val),
            _ => (),
        }
    }
}

impl Powered for Vram {
    fn reset(&mut self) {
        self.buffer = 0;
    }
    fn power_cycle(&mut self) {
        self.reset();
    }
}

impl Savable for Vram {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.nametable.save(fh)?;
        self.palette.save(fh)?;
        // Ignore mapper
        self.buffer.save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.nametable.load(fh)?;
        self.palette.load(fh)?;
        self.buffer.load(fh)?;
        Ok(())
    }
}
