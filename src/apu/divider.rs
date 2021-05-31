use crate::{
    common::{Clocked, Powered},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

#[derive(Debug, Copy, Clone)]
pub struct Divider {
    pub counter: f32,
    pub period: f32,
}

impl Divider {
    pub(super) fn new(period: f32) -> Self {
        Self {
            counter: period,
            period,
        }
    }
}

impl Clocked for Divider {
    fn clock(&mut self) -> usize {
        if self.counter > 0.0 {
            self.counter -= 1.0;
        }
        if self.counter <= 0.0 {
            // Reset and output a clock
            self.counter += self.period;
            1
        } else {
            0
        }
    }
}

impl Powered for Divider {
    fn reset(&mut self) {
        self.counter = self.period;
    }
}

impl Savable for Divider {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.counter.save(fh)?;
        self.period.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.counter.load(fh)?;
        self.period.load(fh)?;
        Ok(())
    }
}
