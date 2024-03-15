use crate::nes::event::EmulationEvent;
use anyhow::Context;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};
use tetanes_core::cpu::Cpu;
use tetanes_util::{filesystem, NesResult};
use tracing::{info, warn};

#[derive(Debug, Serialize, Deserialize)]
pub struct State((Cpu, Vec<ReplayEvent>));

#[derive(Debug, Serialize, Deserialize)]
#[must_use]
pub struct ReplayEvent {
    pub frame: u32,
    pub event: EmulationEvent,
}

/// Represents which mode the emulator is in for the Replay feature.
#[derive(Default, Debug)]
pub enum Mode {
    #[default]
    Off,
    Recording(State),
    Playback(State),
}

#[derive(Default, Debug)]
#[must_use]
pub struct Replay {
    pub mode: Mode,
}

impl Replay {
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        matches!(self.mode, Mode::Recording(..))
    }

    #[must_use]
    pub const fn is_playing(&self) -> bool {
        matches!(self.mode, Mode::Playback(..))
    }

    pub fn start(&mut self, cpu: Cpu) {
        self.mode = Mode::Recording(State((cpu, vec![])));
    }

    pub fn stop(&mut self) -> NesResult<()> {
        if let Mode::Recording(State((cpu, events))) = std::mem::take(&mut self.mode) {
            self.save(cpu, events)
        } else {
            Ok(())
        }
    }

    pub fn toggle(&mut self, cpu: &Cpu) -> NesResult<()> {
        if let Mode::Recording(State((cpu, events))) = std::mem::take(&mut self.mode) {
            self.save(cpu, events)?;
            self.stop()
        } else {
            self.start(cpu.clone());
            Ok(())
        }
    }

    pub fn record(&mut self, frame: u32, event: EmulationEvent) {
        if let Mode::Recording(State((_, ref mut events))) = self.mode {
            if matches!(
                event,
                EmulationEvent::Joypad(..) | EmulationEvent::ZapperTrigger
            ) {
                events.push(ReplayEvent { frame, event });
            }
        }
    }

    /// Saves the replay recording out to a file.
    pub fn save(&self, cpu: Cpu, events: Vec<ReplayEvent>) -> NesResult<()> {
        let replay_path = PathBuf::from(
            Local::now()
                .format("tetanes_replay_%Y-%m-%d_%H.%M.%S")
                .to_string(),
        )
        .with_extension("replay");
        info!("saving replay to {replay_path:?}...",);
        bincode::serialize(&State((cpu, events)))
            .context("failed to serialize replay recording")
            .and_then(|data| filesystem::save_data(replay_path, &data))
    }

    /// Loads a replay recording file.
    pub fn load(&mut self, path: impl AsRef<Path>) -> NesResult<()> {
        let path = path.as_ref();
        info!("loading replay {}...", path.display());
        filesystem::load_data(path).and_then(|data| {
            bincode::deserialize::<State>(&data)
                .context("failed to deserialize replay recording")
                .map(|State((cpu, mut events))| {
                    events.reverse(); // So we can pop off the end
                    self.mode = Mode::Playback(State((cpu, events)));
                })
        })
    }

    pub fn next(&mut self, frame: u32) -> Option<EmulationEvent> {
        if let Mode::Playback(State((_, ref mut events))) = self.mode {
            match events.last() {
                Some(event) => match event.frame.cmp(&frame) {
                    Ordering::Less | Ordering::Equal => {
                        if event.frame < frame {
                            warn!("out of order replay event: {} < {frame}", event.frame);
                        }
                        return events.pop().map(|event| event.event);
                    }
                    Ordering::Greater => (),
                },
                None => self.mode = Mode::Off,
            }
        }
        None
    }
}
