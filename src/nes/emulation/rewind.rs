use crate::{cpu::Cpu, nes::emulation::State};
use std::collections::VecDeque;
use winit::event::ElementState;

#[derive(Default, Debug)]
#[must_use]
pub struct Rewind {
    frames: u8,
    interval: u8,
    max_buffer_size: usize,
    enabled: bool,
    buffer: VecDeque<Cpu>,
}

impl Rewind {
    pub fn new(enabled: bool, interval: u8, max_buffer_size: usize) -> Self {
        Self {
            frames: 0,
            interval,
            max_buffer_size,
            enabled,
            buffer: if enabled {
                VecDeque::with_capacity(max_buffer_size)
            } else {
                VecDeque::new()
            },
        }
    }

    pub fn enable(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn push(&mut self, cpu: &Cpu) {
        if !self.enabled {
            return;
        }
        self.frames += 1;
        if self.frames >= self.interval {
            self.frames = 0;
            self.buffer.push_front(cpu.clone());
            let buffer_size = self
                .buffer
                .iter()
                .fold(0, |size, _| size + std::mem::size_of::<Cpu>());
            if buffer_size >= self.max_buffer_size {
                self.buffer.pop_back();
            }
        }
    }

    pub fn pop(&mut self) -> Option<Cpu> {
        self.buffer.pop_front()
    }
}

impl State {
    pub fn rewind(&mut self) {
        if let Some(cpu) = self.rewind.pop() {
            self.control_deck.load_cpu(cpu);
        }
    }

    pub fn instant_rewind(&mut self) {
        // Two seconds worth of frames @ 60 FPS
        let mut rewind_frames = 120 / self.config.rewind_interval;
        while rewind_frames > 0 {
            self.rewind.buffer.pop_front();
            rewind_frames -= 1;
        }
        self.rewind();
    }

    pub fn on_rewind(&mut self, state: ElementState, repeat: bool) {
        if !self.config.rewind {
            self.add_message("Rewind disabled. You can enable it in the Config menu.");
            return;
        }
        if repeat {
            self.rewinding = true;
            self.pause(true);
        } else if state == ElementState::Released {
            if self.rewinding {
                self.pause(false);
            } else {
                self.instant_rewind();
            }
        }
    }
}
