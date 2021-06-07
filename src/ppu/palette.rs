use crate::{
    memory::{MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

pub(super) const PALETTE_SIZE: usize = 32;
pub(super) const SYSTEM_PALETTE_SIZE: usize = 64;
pub(super) const PALETTE_START: u16 = 0x3F00;
pub(super) const PALETTE_END: u16 = 0x3F20;

// http://wiki.nesdev.com/w/index.php/PPU_palettes
#[derive(Clone)]
pub(super) struct Palette([u8; PALETTE_SIZE]);

impl Palette {
    pub(super) fn new(data: [u8; PALETTE_SIZE]) -> Self {
        Self(data)
    }
}

impl MemRead for Palette {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }
    fn peek(&self, mut addr: u16) -> u8 {
        if addr >= 16 && addr.trailing_zeros() >= 2 {
            addr -= 16;
        }
        self.0[addr as usize]
    }
}
impl MemWrite for Palette {
    fn write(&mut self, mut addr: u16, val: u8) {
        if addr >= 16 && addr.trailing_zeros() >= 2 {
            addr -= 16;
        }
        self.0[addr as usize] = val;
    }
}

impl Savable for Palette {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.0.save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.0.load(fh)
    }
}

// 64 total possible colors, though only 32 can be loaded at a time
#[rustfmt::skip]
pub(crate) const SYSTEM_PALETTE: [u8; SYSTEM_PALETTE_SIZE * 3] = [
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
