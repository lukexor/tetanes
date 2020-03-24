use crate::{common::Clocked, serialization::Savable, NesResult};
use std::io::{Read, Write};

#[derive(Clone)]
pub struct Envelope {
    pub enabled: bool,
    loops: bool,
    pub reset: bool,
    pub volume: u8,
    pub constant_volume: u8,
    counter: u8,
}

impl Envelope {
    pub fn new() -> Self {
        Self {
            enabled: false,
            loops: false,
            reset: false,
            volume: 0u8,
            constant_volume: 0u8,
            counter: 0u8,
        }
    }

    // $4000/$4004/$400C Envelope control
    pub fn write_control(&mut self, val: u8) {
        self.loops = (val >> 5) & 1 == 1; // D5
        self.enabled = (val >> 4) & 1 == 0; // !D4
        self.constant_volume = val & 0x0F; // D3..D0
    }
}

impl Clocked for Envelope {
    fn clock(&mut self) -> usize {
        if self.reset {
            self.reset = false;
            self.volume = 0x0F;
            self.counter = self.constant_volume;
        } else if self.counter > 0 {
            self.counter -= 1;
        } else {
            self.counter = self.constant_volume;
            if self.volume > 0 {
                self.volume -= 1;
            } else if self.loops {
                self.volume = 0x0F;
            }
        }
        1
    }
}

impl Savable for Envelope {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.loops.save(fh)?;
        self.reset.save(fh)?;
        self.volume.save(fh)?;
        self.constant_volume.save(fh)?;
        self.counter.save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.loops.load(fh)?;
        self.reset.load(fh)?;
        self.volume.load(fh)?;
        self.constant_volume.load(fh)?;
        self.counter.load(fh)?;
        Ok(())
    }
}
