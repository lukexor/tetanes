use crate::nes::{config::Config, event::EmulationEvent};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    io::Read,
    path::{Path, PathBuf},
};
use tetanes_core::{cpu::Cpu, fs};
use tracing::warn;

#[derive(Debug, Serialize, Deserialize)]
pub struct State((Cpu, Vec<ReplayEvent>));

#[derive(Debug, Serialize, Deserialize)]
#[must_use]
pub struct ReplayEvent {
    pub frame: u32,
    pub event: EmulationEvent,
}

#[derive(Default, Debug)]
#[must_use]
pub struct Record {
    pub start: Option<Cpu>,
    pub events: Vec<ReplayEvent>,
}

impl Record {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, cpu: Cpu) {
        self.start = Some(cpu);
        self.events.clear();
    }

    pub fn stop(&mut self, name: &str) -> anyhow::Result<Option<PathBuf>> {
        self.save(name)
    }

    pub fn push(&mut self, frame: u32, event: EmulationEvent) {
        if self.start.is_some()
            && matches!(
                event,
                EmulationEvent::Joypad(..) | EmulationEvent::ZapperTrigger
            )
        {
            self.events.push(ReplayEvent { frame, event });
        }
    }

    /// Saves the replay recording out to a file.
    pub fn save(&mut self, name: &str) -> anyhow::Result<Option<PathBuf>> {
        let Some(start) = self.start.take() else {
            tracing::debug!("not saving - replay not started");
            return Ok(None);
        };
        if self.events.is_empty() {
            tracing::debug!("not saving - no replay events");
            return Ok(None);
        }
        if let Some(dir) = Config::default_data_dir() {
            let path = dir
                .join(
                    Local::now()
                        .format(&format!("tetanes_replay_{name}_%Y-%m-%d_%H.%M.%S"))
                        .to_string(),
                )
                .with_extension("replay");
            let events = std::mem::take(&mut self.events);
            fs::save(&path, &State((start, events)))?;
            Ok(Some(path))
        } else {
            Err(anyhow::anyhow!("failed to find document directory"))
        }
    }
}

#[derive(Default, Debug)]
#[must_use]
pub struct Replay {
    pub events: Vec<ReplayEvent>,
}

impl Replay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads a replay recording file.
    pub fn load_path(&mut self, path: impl AsRef<Path>) -> anyhow::Result<Cpu> {
        let path = path.as_ref();
        let State((cpu, mut events)) = fs::load(path)?;
        events.reverse(); // So we can pop off the end
        self.events = events;
        Ok(cpu)
    }

    /// Loads a replay from a reader.
    pub fn load(&mut self, mut replay: impl Read) -> anyhow::Result<Cpu> {
        let mut events = Vec::new();
        replay.read_to_end(&mut events)?;
        let State((cpu, mut events)) = fs::load_bytes(&events)?;
        events.reverse(); // So we can pop off the end
        self.events = events;
        Ok(cpu)
    }

    pub fn next(&mut self, frame: u32) -> Option<EmulationEvent> {
        if let Some(event) = self.events.last() {
            match event.frame.cmp(&frame) {
                Ordering::Less | Ordering::Equal => {
                    if event.frame < frame {
                        warn!("out of order replay event: {} < {frame}", event.frame);
                    }
                    return self.events.pop().map(|event| event.event);
                }
                Ordering::Greater => (),
            }
        }
        None
    }
}
