use crate::{
    common::{Clocked, Powered},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

#[derive(Debug, Clone)]
#[must_use]
pub(crate) struct Sequencer {
    pub(crate) step: usize,
    pub(crate) length: usize,
}

impl Sequencer {
    pub(crate) const fn new(length: usize) -> Self {
        Self { step: 1, length }
    }
}

impl Clocked for Sequencer {
    #[inline]
    fn clock(&mut self) -> usize {
        let clock = self.step;
        self.step += 1;
        if self.step > self.length {
            self.step = 1;
        }
        clock
    }
}

impl Powered for Sequencer {
    fn reset(&mut self) {
        self.step = 1;
    }
}

impl Savable for Sequencer {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.step.save(fh)?;
        self.length.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.step.load(fh)?;
        self.length.load(fh)?;
        Ok(())
    }
}
