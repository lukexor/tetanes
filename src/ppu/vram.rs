use crate::{
    common::Powered,
    mapper::{Mapper, MapperType, Mirroring},
    memory::{MemRead, MemWrite, Memory, RamState},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

// Two 1K nametables exist in hardware
pub const NT_SIZE: usize = 2 * 1024;
// Nametable ranges:
// [ $2000 $2400 ]
// [ $2800 $2C00 ]
pub const NT_START: u16 = 0x2000;
pub const ATTRIBUTE_START: u16 = 0x23C0; // Attributes for NameTables

pub const PALETTE_SIZE: usize = 32;
pub const SYSTEM_PALETTE_SIZE: usize = 64;
pub const PALETTE_START: u16 = 0x3F00;
pub const PALETTE_END: u16 = 0x3F20;

// 64 total possible colors, though only 32 can be loaded at a time
#[rustfmt::skip]
pub const SYSTEM_PALETTE: [u8; SYSTEM_PALETTE_SIZE * 3] = [
    // 0x00
    84, 84, 84,    0, 30, 116,    8, 16, 144,    48, 0, 136,    // $00-$03
    68, 0, 100,    92, 0, 48,     84, 4, 0,      60, 24, 0,     // $04-$07
    32, 42, 0,     8, 58, 0,      0, 64, 0,      0, 60, 0,      // $08-$0B
    0, 50, 60,     0, 0, 0,       0, 0, 0,       0, 0, 0,       // $0C-$0F
    // 0x10
    152, 150, 152, 8, 76, 196,    48, 50, 236,   92, 30, 228,   // $10-$13
    136, 20, 176,  160, 20, 100,  152, 34, 32,   120, 60, 0,    // $14-$17
    84, 90, 0,     40, 114, 0,    8, 124, 0,     0, 118, 40,    // $18-$1B
    0, 102, 120,   0, 0, 0,       0, 0, 0,       0, 0, 0,       // $1C-$1F
    // 0x20
    236, 238, 236, 76, 154, 236,  120, 124, 236, 176, 98, 236,  // $20-$23
    228, 84, 236,  236, 88, 180,  236, 106, 100, 212, 136, 32,  // $24-$27
    160, 170, 0,   116, 196, 0,   76, 208, 32,   56, 204, 108,  // $28-$2B
    56, 180, 204,  60, 60, 60,    0, 0, 0,       0, 0, 0,       // $2C-$2F
    // 0x30
    236, 238, 236, 168, 204, 236, 188, 188, 236, 212, 178, 236, // $30-$33
    236, 174, 236, 236, 174, 212, 236, 180, 176, 228, 196, 144, // $34-$37
    204, 210, 120, 180, 222, 120, 168, 226, 144, 152, 226, 180, // $38-$3B
    160, 214, 228, 160, 162, 160, 0, 0, 0,       0, 0, 0,       // $3C-$3F
];

#[derive(Debug, Clone)]
#[must_use]
pub struct Vram {
    // Used to layout backgrounds on the screen
    // http://wiki.nesdev.com/w/index.php/PPU_nametables
    // http://wiki.nesdev.com/w/index.php/PPU_attribute_tables
    pub nametable: Memory,
    // Background/Sprite color palettes
    // http://wiki.nesdev.com/w/index.php/PPU_palettes
    pub palette: Memory,
    pub mapper: *mut MapperType,
    pub buffer: u8, // PPUDATA buffer
}

impl Default for Vram {
    fn default() -> Self {
        Self::new()
    }
}

impl Vram {
    pub fn new() -> Self {
        Self {
            nametable: Memory::ram(NT_SIZE, RamState::AllZeros),
            palette: Memory::ram(PALETTE_SIZE, RamState::AllZeros),
            mapper: std::ptr::null_mut(),
            buffer: 0u8,
        }
    }

    pub fn nametable_addr(&self, addr: u16) -> u16 {
        let mirroring = self.mapper().mirroring();
        // Maps addresses to nametable pages based on mirroring mode
        let page = match mirroring {
            Mirroring::Horizontal => (addr >> 11) & 1,
            Mirroring::Vertical => (addr >> 10) & 1,
            Mirroring::SingleScreenA => (addr >> 14) & 1,
            Mirroring::SingleScreenB => (addr >> 13) & 1,
            Mirroring::FourScreen => self.mapper().nametable_page(addr),
        };
        let table_size = 0x0400;
        let offset = addr % table_size;
        NT_START + page * table_size + offset
    }

    #[inline]
    pub fn mapper(&self) -> &MapperType {
        unsafe { &*self.mapper }
    }

    #[inline]
    pub fn mapper_mut(&mut self) -> &mut MapperType {
        unsafe { &mut *self.mapper }
    }
}

impl MemRead for Vram {
    fn read(&mut self, mut addr: u16) -> u8 {
        self.mapper_mut().vram_change(addr);
        match addr {
            0x0000..=0x1FFF => self.mapper_mut().read(addr),
            0x2000..=0x3EFF => {
                // Use PPU Nametables or Cartridge RAM
                if self.mapper().use_ciram(addr) {
                    let mirror_addr = self.nametable_addr(addr);
                    self.nametable.read(mirror_addr % NT_SIZE as u16)
                } else {
                    self.mapper_mut().read(addr)
                }
            }
            0x3F00..=0x3FFF => {
                if addr >= 16 && addr.trailing_zeros() >= 2 {
                    addr -= 16;
                }
                self.palette.read(addr % PALETTE_SIZE as u16)
            }
            _ => 0,
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.mapper().peek(addr),
            0x2000..=0x3EFF => {
                // Use PPU Nametables or Cartridge RAM
                if self.mapper().use_ciram(addr) {
                    let mirror_addr = self.nametable_addr(addr);
                    self.nametable.peek(mirror_addr % NT_SIZE as u16)
                } else {
                    self.mapper().peek(addr)
                }
            }
            0x3F00..=0x3FFF => self.palette.peek(addr % PALETTE_SIZE as u16),
            _ => 0,
        }
    }
}

impl MemWrite for Vram {
    fn write(&mut self, mut addr: u16, val: u8) {
        self.mapper_mut().vram_change(addr);
        match addr {
            0x0000..=0x1FFF => self.mapper_mut().write(addr, val),
            0x2000..=0x3EFF => {
                if self.mapper().use_ciram(addr) {
                    let mirror_addr = self.nametable_addr(addr);
                    self.nametable.write(mirror_addr % NT_SIZE as u16, val);
                } else {
                    self.mapper_mut().write(addr, val);
                }
            }
            0x3F00..=0x3FFF => {
                if addr >= 16 && addr.trailing_zeros() >= 2 {
                    addr -= 16;
                }
                self.palette.write(addr % PALETTE_SIZE as u16, val);
            }
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
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.nametable.save(fh)?;
        self.palette.save(fh)?;
        // Ignore mapper
        self.buffer.save(fh)?;
        Ok(())
    }

    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.nametable.load(fh)?;
        self.palette.load(fh)?;
        self.buffer.load(fh)?;
        Ok(())
    }
}
