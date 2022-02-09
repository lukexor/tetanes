use crate::{serialization::Savable, NesResult};
use std::io::{Read, Write};

#[derive(Debug, Clone)]
#[must_use]
pub(crate) struct LinearCounter {
    pub(crate) reload: bool,
    pub(crate) control: bool,
    pub(crate) load: u8,
    pub(crate) counter: u8,
}

impl LinearCounter {
    pub(crate) const fn new() -> Self {
        Self {
            reload: false,
            control: false,
            load: 0u8,
            counter: 0u8,
        }
    }

    #[inline]
    pub(crate) fn load_value(&mut self, val: u8) {
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
