use crate::{serialization::Savable, NesResult};
use std::io::{Read, Write};

#[derive(Debug, Clone)]
pub(crate) struct Sweep {
    pub(crate) enabled: bool,
    pub(crate) reload: bool,
    pub(crate) negate: bool, // Treats PulseChannel 1 differently than PulseChannel 2
    pub(crate) timer: u8,    // counter reload value
    pub(crate) counter: u8,  // current timer value
    pub(crate) shift: u8,
}

impl Savable for Sweep {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.reload.save(fh)?;
        self.negate.save(fh)?;
        self.timer.save(fh)?;
        self.counter.save(fh)?;
        self.shift.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.reload.load(fh)?;
        self.negate.load(fh)?;
        self.timer.load(fh)?;
        self.counter.load(fh)?;
        self.shift.load(fh)?;
        Ok(())
    }
}
