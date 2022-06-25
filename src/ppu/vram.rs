use crate::{
    cart::Cart,
    common::{Kind, Reset},
    mapper::Mapped,
    memory::{MemRead, MemWrite, Memory, RamState},
};
use serde::{Deserialize, Serialize};
use std::fmt;

// Two 1K nametables exist in hardware
pub const VRAM_SIZE: usize = 2 * 1024;
// Nametable ranges:
// [ $2000 $2400 ]
// [ $2800 $2C00 ]
pub const NT_START: u16 = 0x2000;
pub const NT_SIZE: u16 = 0x0400;
pub const ATTR_START: u16 = 0x23C0; // Attributes for NameTables
pub const ATTR_OFFSET: u16 = 0x03C0;

pub const PALETTE_SIZE: usize = 32;
pub const SYSTEM_PALETTE_SIZE: usize = 64;
pub const PALETTE_START: u16 = 0x3F00;
pub const PALETTE_END: u16 = 0x3F20;

// 64 total possible colors, though only 32 can be loaded at a time
#[rustfmt::skip]
pub const SYSTEM_PALETTE: [(u8,u8,u8); SYSTEM_PALETTE_SIZE] = [
    // 0x00
    (0x54, 0x54, 0x54), (0x00, 0x1E, 0x74), (0x08, 0x10, 0x90), (0x30, 0x00, 0x88), // $00-$03
    (0x44, 0x00, 0x64), (0x5C, 0x00, 0x30), (0x54, 0x04, 0x00), (0x3C, 0x18, 0x00), // $04-$07
    (0x20, 0x2A, 0x00), (0x08, 0x3A, 0x00), (0x00, 0x40, 0x00), (0x00, 0x3C, 0x00), // $08-$0B
    (0x00, 0x32, 0x3C), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), // $0C-$0F
    // 0x10
    (0x98, 0x96, 0x98), (0x08, 0x4C, 0xC4), (0x30, 0x32, 0xEC), (0x5C, 0x1E, 0xE4), // $10-$13
    (0x88, 0x14, 0xB0), (0xA0, 0x14, 0x64), (0x98, 0x22, 0x20), (0x78, 0x3C, 0x00), // $14-$17
    (0x54, 0x5A, 0x00), (0x28, 0x72, 0x00), (0x08, 0x7C, 0x00), (0x00, 0x76, 0x28), // $18-$1B
    (0x00, 0x66, 0x78), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), // $1C-$1F
    // 0x20
    (0xEC, 0xEE, 0xEC), (0x4C, 0x9A, 0xEC), (0x78, 0x7C, 0xEC), (0xB0, 0x62, 0xEC), // $20-$23
    (0xE4, 0x54, 0xEC), (0xEC, 0x58, 0xB4), (0xEC, 0x6A, 0x64), (0xD4, 0x88, 0x20), // $24-$27
    (0xA0, 0xAA, 0x00), (0x74, 0xC4, 0x00), (0x4C, 0xD0, 0x20), (0x38, 0xCC, 0x6C), // $28-$2B
    (0x38, 0xB4, 0xCC), (0x3C, 0x3C, 0x3C), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), // $2C-$2F
    // 0x30
    (0xEC, 0xEE, 0xEC), (0xA8, 0xCC, 0xEC), (0xBC, 0xBC, 0xEC), (0xD4, 0xB2, 0xEC), // $30-$33
    (0xEC, 0xAE, 0xEC), (0xEC, 0xAE, 0xD4), (0xEC, 0xB4, 0xB0), (0xE4, 0xC4, 0x90), // $34-$37
    (0xCC, 0xD2, 0x78), (0xB4, 0xDE, 0x78), (0xA8, 0xE2, 0x90), (0x98, 0xE2, 0xB4), // $38-$3B
    (0xA0, 0xD6, 0xE4), (0xA0, 0xA2, 0xA0), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), // $3C-$3F
];

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vram {
    // Used to layout backgrounds on the screen
    // http://wiki.nesdev.com/w/index.php/PPU_nametables
    // http://wiki.nesdev.com/w/index.php/PPU_attribute_tables
    pub nametable: Memory,
    // Background/Sprite color palettes
    // http://wiki.nesdev.com/w/index.php/PPU_palettes
    pub palette: Memory,
    #[serde(skip, default = "std::ptr::null_mut")]
    pub cart: *mut Cart,
    pub buffer: u8, // PPUDATA buffer
}

impl Vram {
    pub fn new() -> Self {
        Self {
            nametable: Memory::ram(VRAM_SIZE, RamState::AllZeros),
            palette: Memory::ram(PALETTE_SIZE, RamState::AllZeros),
            cart: std::ptr::null_mut(),
            buffer: 0u8,
        }
    }

    #[must_use]
    pub(crate) fn nametable_addr(&self, addr: u16) -> u16 {
        // Maps addresses to nametable pages based on mirroring mode
        let page = self.cart().nametable_page(addr).unwrap_or(0);
        let offset = addr % NT_SIZE;
        NT_START + page * NT_SIZE + offset
    }

    #[allow(clippy::missing_const_for_fn)]
    #[inline]
    pub(crate) fn cart(&self) -> &Cart {
        assert!(!self.cart.is_null(), "VRAM cart reference is null");
        unsafe { &*self.cart }
    }

    #[inline]
    pub(crate) fn cart_mut(&mut self) -> &mut Cart {
        assert!(!self.cart.is_null(), "VRAM cart reference is null");
        unsafe { &mut *self.cart }
    }
}

impl MemRead for Vram {
    fn read(&mut self, addr: u16) -> u8 {
        self.cart_mut().ppu_read(addr);
        match addr {
            0x0000..=0x1FFF => self.cart_mut().read(addr),
            0x2000..=0x3EFF => {
                // Use PPU Nametables or Cartridge RAM
                if self.cart().use_ciram(addr) {
                    let mirror_addr = self.nametable_addr(addr);
                    self.nametable.read(mirror_addr)
                } else {
                    self.cart_mut().read(addr)
                }
            }
            0x3F00..=0x3FFF => {
                let mut addr = addr & 0x1F;
                if addr >= 16 && addr.trailing_zeros() >= 2 {
                    addr -= 16;
                }
                self.palette.read(addr)
            }
            _ => 0x00,
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cart().peek(addr),
            0x2000..=0x3EFF => {
                // Use PPU Nametables or Cartridge RAM
                if self.cart().use_ciram(addr) {
                    let mirror_addr = self.nametable_addr(addr);
                    self.nametable.peek(mirror_addr)
                } else {
                    self.cart().peek(addr)
                }
            }
            0x3F00..=0x3FFF => self.palette.peek(addr),
            _ => 0x00,
        }
    }
}

impl MemWrite for Vram {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => self.cart_mut().write(addr, val),
            0x2000..=0x3EFF => {
                if self.cart().use_ciram(addr) {
                    let mirror_addr = self.nametable_addr(addr);
                    self.nametable.write(mirror_addr, val);
                } else {
                    self.cart_mut().write(addr, val);
                }
            }
            0x3F00..=0x3FFF => {
                let mut addr = addr % PALETTE_SIZE as u16;
                if addr >= 16 && addr.trailing_zeros() >= 2 {
                    addr -= 16;
                }
                self.palette.write(addr, val);
            }
            _ => (),
        }
    }
}

impl Reset for Vram {
    fn reset(&mut self, _kind: Kind) {
        self.buffer = 0;
    }
}

impl Default for Vram {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Vram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vram")
            .field("nametable", &self.nametable)
            .field("palette", &self.palette)
            .field("cart", &format_args!("{:p}", self.cart))
            .field("buffer", &format_args!("${:02X}", &self.buffer))
            .finish()
    }
}
