use crate::nes::{emulation::State, renderer::gui::MessageType};
use tetanes_core::{
    cpu::Cpu,
    fs::{Error, Result},
    ppu::frame::Buffer,
};
use tracing::error;

#[derive(Default, Debug, Clone)]
#[must_use]
pub(crate) struct Frame {
    pub(crate) buffer: Buffer,
    pub(crate) state: Vec<u8>,
}

#[derive(Default, Debug)]
#[must_use]
pub(crate) struct Rewind {
    pub(crate) enabled: bool,
    pub(crate) interval_counter: usize,
    pub(crate) index: usize,
    pub(crate) count: usize,
    pub(crate) interval: usize,
    pub(crate) seconds: usize,
    pub(crate) frames: Vec<Option<Frame>>,
}

impl Rewind {
    const TARGET_FPS: usize = 60;

    pub(crate) fn new(enabled: bool, seconds: u32, interval: u32) -> Self {
        let interval = interval as usize;
        let seconds = seconds as usize;
        Self {
            enabled,
            interval_counter: 0,
            index: 0,
            count: 0,
            interval,
            seconds,
            frames: vec![None; Self::frame_size(seconds, interval)],
        }
    }

    const fn frame_size(seconds: usize, interval: usize) -> usize {
        Self::TARGET_FPS * seconds / interval
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    pub(crate) fn set_seconds(&mut self, seconds: u32) {
        self.seconds = seconds as usize;
        self.frames
            .resize(Self::frame_size(self.seconds, self.interval), None);
    }

    pub(crate) fn set_interval(&mut self, interval: u32) {
        self.interval = interval as usize;
        self.frames
            .resize(Self::frame_size(self.seconds, self.interval), None);
    }

    pub(crate) fn push(&mut self, cpu: &Cpu) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.interval_counter += 1;
        if self.interval_counter >= self.interval {
            self.interval_counter = 0;

            let config = bincode::config::legacy();
            let state = bincode::serde::encode_to_vec(cpu, config)
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

    pub(crate) fn pop(&mut self) -> Option<Cpu> {
        if !self.enabled {
            return None;
        }
        if self.count > 0 {
            self.count -= 1;
            self.index -= 1;
            if self.index == 0 {
                self.index = self.frames.len().saturating_sub(1);
            }

            let frame = self.frames[self.index].take()?;
            let config = bincode::config::legacy();
            bincode::serde::decode_from_slice::<Cpu, _>(&frame.state, config)
                .map(|(mut cpu, _)| {
                    cpu.bus.input.clear(); // Discard inputs while rewinding
                    cpu.bus.ppu.frame.buffer = frame.buffer;
                    cpu
                })
                .map_err(|err| error!("Failed to deserialize CPU state: {err:?}"))
                .ok()
        } else {
            None
        }
    }

    pub(crate) fn clear(&mut self) {
        self.interval_counter = 0;
        self.index = 0;
        self.count = 0;
        self.frames.fill(None);
    }
}

impl State {
    pub(crate) fn rewind_disabled(&mut self) {
        self.add_message(
            MessageType::Warn,
            "Rewind disabled. You can enable it in the Preferences menu.",
        );
    }

    pub(crate) fn instant_rewind(&mut self) {
        if !self.rewind.enabled {
            return self.rewind_disabled();
        }
        // ~2 seconds worth of frames @ 60 FPS
        let mut rewind_frames = 120 / self.rewind.interval;
        while let Some(mut cpu) = self.rewind.pop() {
            cpu.bus.input.clear(); // Discard inputs while rewinding
            self.control_deck.load_cpu(cpu);
            rewind_frames -= 1;
            if rewind_frames == 0 {
                break;
            }
        }
    }
}
