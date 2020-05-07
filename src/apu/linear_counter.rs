use crate::{serialization::Savable, NesResult};
use std::io::{Read, Write};

#[derive(Debug, Clone)]
pub struct LinearCounter {
    pub reload: bool,
    pub control: bool,
    pub load: u8,
    pub counter: u8,
}

impl LinearCounter {
    pub fn new() -> Self {
        Self {
            reload: false,
            control: false,
            load: 0u8,
            counter: 0u8,
        }
    }

    pub fn load_value(&mut self, val: u8) {
        self.load = val >> 1; // D6..D0
    }
}

impl Savable for LinearCounter {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.reload.save(fh)?;
        self.control.save(fh)?;
        self.load.save(fh)?;
        self.counter.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.reload.load(fh)?;
        self.control.load(fh)?;
        self.load.load(fh)?;
        self.counter.load(fh)?;
        Ok(())
    }
}

impl Default for LinearCounter {
    fn default() -> Self {
        Self::new()
    }
}
