use crate::nes::emulation::State;
use tetanes_core::cpu::Cpu;

#[derive(Default, Debug)]
#[must_use]
pub struct Rewind {
    frames: u8,
    index: usize,
    count: usize,
    buffer: Vec<Option<Cpu>>,
}

impl Rewind {
    const BUFFER_SIZE: usize = 2048; // ~34 seconds of frames
    const INTERVAL: u8 = 2;

    pub fn new() -> Self {
        Self {
            frames: 0,
            index: 0,
            count: 0,
            buffer: vec![None; Self::BUFFER_SIZE],
        }
    }

    pub fn push(&mut self, cpu: &Cpu) {
        self.frames += 1;
        if self.frames >= Self::INTERVAL {
            self.frames = 0;
            let mut cpu = cpu.clone();
            cpu.bus.prg_rom.clear();
            cpu.bus.ppu.bus.chr_rom.clear();
            cpu.bus.input.clear();
            self.buffer[self.index] = Some(cpu);
            self.index += 1;
            self.count += 1;
            if self.index >= self.buffer.len() {
                self.index = 0;
            }
        }
    }

    pub fn pop(&mut self) -> Option<Cpu> {
        if self.count > 0 {
            let cpu = self.buffer[self.index].take();
            self.index -= 1;
            self.count -= 1;
            if self.index == 0 {
                self.index = self.buffer.len() - 1;
            }
            cpu
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
        if !self.config.read(|cfg| cfg.emulation.rewind) {
            return self.rewind_disabled();
        }
        if let Some(ref mut rewind) = self.rewind {
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
    }
}
