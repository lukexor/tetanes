use crate::{
    memory::{MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

pub(crate) const OAM_SIZE: usize = 64 * 4; // 64 entries * 4 bytes each

// Addr Low Nibble
// $00, $04, $08, $0C   Sprite Y coord
// $01, $05, $09, $0D   Sprite tile #
// $02, $06, $0A, $0E   Sprite attribute
// $03, $07, $0B, $0F   Sprite X coord
#[derive(Clone)]
pub(crate) struct Oam([u8; OAM_SIZE]);

impl Oam {
    pub(crate) const fn new() -> Self {
        Self([0; OAM_SIZE])
    }
}

impl MemRead for Oam {
    #[inline]
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    #[inline]
    fn peek(&self, addr: u16) -> u8 {
        let val = self.0[addr as usize];
        // Bits 2-4 of Sprite attribute should always be 0
        if let 0x02 | 0x06 | 0x0A | 0x0E = addr & 0x0F {
            val & 0xE3
        } else {
            val
        }
    }
}
impl MemWrite for Oam {
    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        self.0[addr as usize] = val;
    }
}

impl Savable for Oam {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.0.save(fh)
    }

    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.0.load(fh)
    }
}
