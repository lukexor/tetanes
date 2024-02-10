use crate::nes::{filesystem, Nes};
use anyhow::Context;
use std::collections::VecDeque;

#[derive(Default, Debug)]
#[must_use]
pub struct State {
    frame: u32,
    buffer: VecDeque<Vec<u8>>,
}

impl Nes {
    pub fn update_rewind(&mut self) {
        if !self.config.rewind {
            return;
        }

        self.rewind_state.frame = self.rewind_state.frame.wrapping_add(1);
        if self.rewind_state.frame >= self.config.rewind_frames {
            self.rewind_state.frame = 0;
            if let Err(err) = bincode::serialize(self.control_deck.cpu())
                .context("failed to serialize rewind state")
                .and_then(|data| filesystem::encode_data(&data))
                .map(|data| self.rewind_state.buffer.push_front(data))
            {
                log::error!("{err:?}");
                self.config.rewind = false;
                self.rewind_state.buffer.clear();
                return;
            }
            let buffer_size = self
                .rewind_state
                .buffer
                .iter()
                .fold(0, |size, data| size + data.len());
            if buffer_size > self.config.rewind_buffer_size * 1024 * 1024 {
                self.rewind_state
                    .buffer
                    .truncate(self.rewind_state.buffer.len() / 2);
            }
        }
    }

    pub fn rewind(&mut self) {
        if let Some(data) = self.rewind_state.buffer.pop_front() {
            if let Err(err) = filesystem::decode_data(&data).and_then(|data| {
                bincode::deserialize(&data)
                    .context("failed to deserialize rewind state")
                    .map(|cpu| self.control_deck.load_cpu(cpu))
            }) {
                log::error!("{err:?}");
                self.config.rewind = false;
                self.rewind_state.buffer.clear();
            }
        }
    }

    pub fn instant_rewind(&mut self) {
        if self.config.rewind {
            // Two seconds worth of frames @ 60 FPS
            let mut rewind_frames = 120 / self.config.rewind_frames as usize;
            while rewind_frames > 0 {
                self.rewind_state.buffer.pop_front();
                rewind_frames -= 1;
            }

            if let Some(data) = self.rewind_state.buffer.pop_front() {
                self.add_message("Rewind");
                if let Err(err) = filesystem::decode_data(&data).and_then(|data| {
                    bincode::deserialize(&data)
                        .context("failed to deserialize rewind state")
                        .map(|cpu| self.control_deck.load_cpu(cpu))
                }) {
                    log::error!("{err:?}");
                    self.config.rewind = false;
                    self.rewind_state.buffer.clear();
                }
            }
        } else {
            self.add_message("Rewind disabled. You can enable it in the Config menu.");
        }
    }
}
