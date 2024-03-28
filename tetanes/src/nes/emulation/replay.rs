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

#[derive(Debug)]
#[must_use]
pub struct Record {
    pub start: Cpu,
    pub events: Vec<ReplayEvent>,
}

impl Record {
    pub fn start(cpu: Cpu) -> Self {
        Self {
            start: cpu,
            events: vec![],
        }
    }

    pub fn stop(self) -> anyhow::Result<PathBuf> {
        self.save()
    }

    pub fn record(&mut self, frame: u32, event: EmulationEvent) {
        if matches!(
            event,
            EmulationEvent::Joypad(..) | EmulationEvent::ZapperTrigger
        ) {
            self.events.push(ReplayEvent { frame, event });
        }
    }

    /// Saves the replay recording out to a file.
    pub fn save(self) -> anyhow::Result<PathBuf> {
        if let Some(dir) = Config::document_dir() {
            let path = dir
                .join(
                    Local::now()
                        .format("tetanes_replay_%Y-%m-%d_%H.%M.%S")
                        .to_string(),
                )
                .with_extension("replay");
            fs::save(&path, &State((self.start, self.events)))?;
            Ok(path)
        } else {
            Err(anyhow::anyhow!("failed to find document directory"))
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct Replay {
    pub events: Vec<ReplayEvent>,
}

impl Replay {
    /// Loads a replay recording file.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<(Cpu, Self)> {
        let path = path.as_ref();
        Ok(fs::load(path).map(|State((cpu, mut events))| {
            events.reverse(); // So we can pop off the end
            (cpu, Self { events })
        })?)
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
