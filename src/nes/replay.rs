use crate::{
    cpu::Cpu,
    nes::{event::ActionEvent, filesystem, Nes},
};
use anyhow::Context;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, path::PathBuf};

/// Represents which mode the emulator is in for the Replay feature.
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Off,
    Recording,
    Playback,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct State {
    pub mode: Mode,
    pub start: Option<Cpu>,
    pub buffer: Vec<ActionEvent>,
}

impl State {
    #[inline]
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        matches!(self.mode, Mode::Recording)
    }

    #[inline]
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        matches!(self.mode, Mode::Playback)
    }
}

impl Nes {
    pub fn start_replay(&mut self) {
        self.replay_state.start = Some(self.control_deck.cpu().clone());
        self.replay_state.mode = Mode::Recording;
        self.add_message("Replay Recording Started");
    }

    pub fn stop_replay(&mut self) {
        if self.replay_state.is_playing() {
            self.add_message("Replay Playback Stopped");
        } else {
            self.add_message("Replay Recording Stopped");
            self.save_replay();
        }
        self.replay_state.mode = Mode::Off;
    }

    /// Saves the replay buffer out to a file
    pub fn save_replay(&mut self) {
        let replay_path = PathBuf::from(
            Local::now()
                .format("tetanes_replay_%Y-%m-%d_%H.%M.%S")
                .to_string(),
        )
        .with_extension("replay");
        log::info!("saving replay to {replay_path:?}...",);
        self.replay_state.buffer.reverse();
        match bincode::serialize(&self.replay_state)
            .context("failed to serialize replay recording")
            .map(|data| filesystem::save_data(replay_path, &data))
        {
            Ok(_) => {
                self.replay_state.buffer.clear();
                self.replay_state.start = None;
                self.add_message("Saved replay recording");
            }
            Err(err) => {
                log::error!("{err:?}");
                self.add_message("Failed to save replay recording");
            }
        }
    }

    /// Loads a replay file
    pub fn load_replay(&mut self) {
        if let Some(replay_path) = &self.config.replay_path {
            log::info!("loading replay {replay_path:?}...",);
            match filesystem::load_data(replay_path).and_then(|data| {
                bincode::deserialize::<State>(&data)
                    .context("failed to deserialize replay recording")
                    .map(|mut replay| {
                        self.control_deck
                            .load_cpu(replay.start.take().expect("valid replay start"));
                        self.replay_state = replay;
                        self.replay_state.mode = Mode::Playback;
                    })
            }) {
                Ok(_) => self.add_message("Loaded replay recording"),
                Err(err) => {
                    log::error!("{err:?}");
                    self.add_message("Failed to load replay recording");
                }
            }
        }
    }

    pub fn replay_action(&mut self) {
        let current_frame = self.control_deck.frame_number();
        while let Some(action_event) = self.replay_state.buffer.last() {
            match action_event.frame.cmp(&current_frame) {
                Ordering::Equal => {
                    let ActionEvent {
                        player,
                        action,
                        state,
                        repeat,
                        ..
                    } = self.replay_state.buffer.pop().expect("valid action event");
                    self.handle_action(player, action, state, repeat);
                }
                Ordering::Less => {
                    log::warn!(
                        "Encountered action event out of order: {} < {}",
                        action_event.frame,
                        current_frame
                    );
                    self.replay_state.buffer.pop();
                }
                Ordering::Greater => break,
            }
        }
        if self.replay_state.buffer.is_empty() {
            self.stop_replay();
        }
    }
}
