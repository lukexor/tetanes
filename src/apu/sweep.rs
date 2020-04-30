use crate::{serialization::Savable, NesResult};
use std::io::{Read, Write};

#[derive(Clone)]
pub struct Sweep {
    pub enabled: bool,
    pub reload: bool,
    pub negate: bool, // Treats PulseChannel 1 differently than PulseChannel 2
    pub timer: u8,    // counter reload value
    pub counter: u8,  // current timer value
    pub shift: u8,
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
