use crate::{common::Clocked, serialization::Savable, NesResult};
use std::io::{Read, Write};

#[derive(Debug, Clone)]
pub struct LengthCounter {
    pub enabled: bool,
    pub counter: u8, // Entry into LENGTH_TABLE
}

impl LengthCounter {
    const LENGTH_TABLE: [u8; 32] = [
        10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96,
        22, 192, 24, 72, 26, 16, 28, 32, 30,
    ];

    pub fn new() -> Self {
        Self {
            enabled: false,
            counter: 0u8,
        }
    }

    pub fn load_value(&mut self, val: u8) {
        self.counter = Self::LENGTH_TABLE[(val >> 3) as usize]; // D7..D3
    }

    pub fn write_control(&mut self, val: u8) {
        self.enabled = (val >> 5) & 1 == 0; // !D5
    }
}

impl Clocked for LengthCounter {
    fn clock(&mut self) -> usize {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
            1
        } else {
            0
        }
    }
}

impl Savable for LengthCounter {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.counter.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.counter.load(fh)?;
        Ok(())
    }
}

impl Default for LengthCounter {
    fn default() -> Self {
        Self::new()
    }
}
