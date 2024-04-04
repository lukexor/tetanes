use crate::nes::emulation::State;
use tetanes_core::{
    cpu::Cpu,
    fs::{Error, Result},
    ppu::frame::Buffer,
};
use tracing::error;

#[derive(Default, Debug, Clone)]
#[must_use]
struct Frame {
    buffer: Buffer,
    state: Vec<u8>,
}

#[derive(Default, Debug)]
#[must_use]
pub struct Rewind {
    enabled: bool,
    interval_counter: u8,
    index: usize,
    count: usize,
    frames: Vec<Option<Frame>>,
}

impl Rewind {
    const FRAMES_SIZE: usize = 1024; // ~34 seconds of frames at a 2 frame interval
    const INTERVAL: u8 = 2;

    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            interval_counter: 0,
            index: 0,
            count: 0,
            frames: vec![None; Self::FRAMES_SIZE],
        }
    }

    pub fn enable(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn push(&mut self, cpu: &Cpu) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.interval_counter += 1;
        if self.interval_counter >= Self::INTERVAL {
            self.interval_counter = 0;

            let state = bincode::serialize(&cpu)
                .map_err(|err| Error::SerializationFailed(err.to_string()))?;
            self.frames[self.index] = Some(Frame {
                buffer: cpu.bus.ppu.frame.buffer.clone(),
                state,
            });

            self.count += 1;
            self.index += 1;
            if self.index >= self.frames.len() {
                self.index = 0;
            }
        }
        Ok(())
    }

    pub fn pop(&mut self) -> Option<Cpu> {
        if !self.enabled {
            return None;
        }
        if self.count > 0 {
            self.count -= 1;
            self.index -= 1;
            if self.index == 0 {
                self.index = self.frames.len() - 1;
            }

            let frame = self.frames[self.index].take()?;
            bincode::deserialize::<Cpu>(&frame.state)
                .map(|mut cpu| {
                    cpu.bus.input.clear();
                    cpu.bus.ppu.frame.buffer = frame.buffer;
                    cpu
                })
                .map_err(|err| error!("Failed to deserialize CPU state: {err:?}"))
                .ok()
        } else {
            None
        }
    }
}

impl State {
    pub fn rewind_disabled(&mut self) {
        self.add_message("Rewind disabled. You can enable it in the Preferences menu.");
    }

    pub fn instant_rewind(&mut self) {
        if !self.rewind.enabled {
            return self.rewind_disabled();
        }
        // Two seconds worth of frames @ 60 FPS
        let mut rewind_frames = 120 / Rewind::INTERVAL;
        while let Some(cpu) = self.rewind.pop() {
            self.control_deck.load_cpu(cpu);
            rewind_frames -= 1;
            if rewind_frames == 0 {
                break;
            }
        }
    }
}
