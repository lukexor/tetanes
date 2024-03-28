use crate::nes::emulation::State;
use tetanes_core::cpu::Cpu;

#[derive(Default, Debug)]
#[must_use]
pub struct Rewind {
    frames: u8,
    index: usize,
    count: usize,
    buffer: Vec<Cpu>,
}

impl Rewind {
    const BUFFER_SIZE: usize = 2048;
    const INTERVAL: u8 = 2;

    pub fn new() -> Self {
        Self {
            frames: 0,
            index: 0,
            count: 0,
            buffer: vec![Cpu::default(); Self::BUFFER_SIZE],
        }
    }

    pub fn push(&mut self, cpu: &Cpu) {
        self.frames += 1;
        if self.frames >= Self::INTERVAL {
            self.frames = 0;
            self.buffer[self.index] = cpu.clone();
            self.index += 1;
            self.count += 1;
            if self.index >= self.buffer.len() {
                self.index = 0;
            }
        }
    }

    pub fn pop(&mut self) -> Option<Cpu> {
        if self.count > 0 {
            let cpu = self.buffer[self.index].clone();
            self.index -= 1;
            if self.index == 0 {
                self.index = self.buffer.len() - 1;
            }
            Some(cpu)
        } else {
            None
        }
    }
}

impl State {
    fn rewind_disabled(&mut self) {
        self.add_message("Rewind disabled. You can enable it in the Preferences menu.");
    }

    pub fn rewind(&mut self) {
        match self.rewind.as_mut().and_then(|r| r.pop()) {
            Some(cpu) => {
                self.control_deck.load_cpu(cpu);
            }
            None => self.rewind_disabled(),
        }
    }

    pub fn instant_rewind(&mut self) {
        match self.rewind {
            Some(ref mut rewind) => {
                // Two seconds worth of frames @ 60 FPS
                let mut rewind_frames = 120 / Rewind::INTERVAL;
                while let Some(cpu) = rewind.pop() {
                    self.control_deck.load_cpu(cpu);
                    rewind_frames -= 1;
                    if rewind_frames == 0 {
                        break;
                    }
                }
            }
            None => self.rewind_disabled(),
        }
    }
}
