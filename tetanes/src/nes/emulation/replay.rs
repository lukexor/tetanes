use crate::nes::{config::Config, event::EmulationEvent};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
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

    pub fn stop(&mut self) -> anyhow::Result<Option<PathBuf>> {
        self.save()
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
    pub fn save(&mut self) -> anyhow::Result<Option<PathBuf>> {
        let Some(start) = self.start.take() else {
            return Ok(None);
        };
        if self.events.is_empty() {
            return Ok(None);
        }
        if let Some(dir) = Config::document_dir() {
            let path = dir
                .join(
                    Local::now()
                        .format("tetanes_replay_%Y-%m-%d_%H.%M.%S")
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
    pub fn load(&mut self, path: impl AsRef<Path>) -> anyhow::Result<Cpu> {
        let path = path.as_ref();
        let State((cpu, mut events)) = fs::load(path)?;
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