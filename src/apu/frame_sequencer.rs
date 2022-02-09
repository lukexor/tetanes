use super::{divider::Divider, sequencer::Sequencer};
use crate::{
    common::{Clocked, Powered},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

#[derive(Debug, Clone)]
pub(crate) struct FrameSequencer {
    pub(crate) divider: Divider,
    pub(crate) sequencer: Sequencer,
    pub(crate) mode: FcMode,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum FcMode {
    Step4,
    Step5,
}

impl FrameSequencer {
    pub(super) const fn new() -> Self {
        Self {
            divider: Divider::new(7457.5),
            sequencer: Sequencer::new(4),
            mode: FcMode::Step4,
        }
    }

    // On write to $4017
    pub(super) fn reload(&mut self, val: u8) {
        // Reset & Configure divider/sequencer
        self.divider.reset();
        self.sequencer = if val & 0x80 == 0x00 {
            self.mode = FcMode::Step4;
            Sequencer::new(4)
        } else {
            self.mode = FcMode::Step5;
            let mut sequencer = Sequencer::new(5);
            let _ = sequencer.clock(); // Clock immediately
            sequencer
        };
    }
}

impl Clocked for FrameSequencer {
    fn clock(&mut self) -> usize {
        // Clocks at 240Hz
        // or 21_477_270 Hz / 89_490
        if self.divider.clock() == 1 {
            self.sequencer.clock()
        } else {
            0
        }
    }
}

impl Powered for FrameSequencer {
    fn reset(&mut self) {
        self.divider.reset();
        self.sequencer.reset();
        self.mode = FcMode::Step4;
    }
}

impl Savable for FrameSequencer {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.divider.save(fh)?;
        self.sequencer.save(fh)?;
        self.mode.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.divider.load(fh)?;
        self.sequencer.load(fh)?;
        self.mode.load(fh)?;
        Ok(())
    }
}

impl Savable for FcMode {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => FcMode::Step4,
            1 => FcMode::Step5,
            _ => panic!("invalid FcMode value"),
        };
        Ok(())
    }
}
