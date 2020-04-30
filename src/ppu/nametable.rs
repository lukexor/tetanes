use crate::{
    memory::{MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

pub(super) const NT_SIZE: usize = 2 * 1024; // Two 1K nametables exist in hardware
                                            // Nametable ranges
                                            // $2000 upper-left corner, $2400 upper-right, $2800 lower-left, $2C00 lower-right
pub(super) const NT_START: u16 = 0x2000;
pub(super) const ATTRIBUTE_START: u16 = 0x23C0; // Attributes for NAMETABLEs

// http://wiki.nesdev.com/w/index.php/PPU_nametables
// http://wiki.nesdev.com/w/index.php/PPU_attribute_tables
#[derive(Clone)]
pub(super) struct Nametable([u8; NT_SIZE]);

impl Nametable {
    pub(super) fn new(data: [u8; NT_SIZE]) -> Self {
        Self(data)
    }
}

impl MemRead for Nametable {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }
    fn peek(&self, addr: u16) -> u8 {
        self.0[addr as usize]
    }
}
impl MemWrite for Nametable {
    fn write(&mut self, addr: u16, val: u8) {
        self.0[addr as usize] = val;
    }
}

impl Savable for Nametable {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.0.save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.0.load(fh)
    }
}
