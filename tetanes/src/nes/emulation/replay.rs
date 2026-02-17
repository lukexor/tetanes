use crate::nes::{config::Config, event::EmulationEvent};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};
use tetanes_core::{
    cpu::Cpu,
    fs,
    input::{JoypadBtn, Player},
};
use tracing::warn;
use winit::event::ElementState;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct State((Cpu, Vec<ReplayFrame>));

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum ReplayEvent {
    Joypad((Player, JoypadBtn, ElementState)),
    ZapperAim((u16, u16)),
    ZapperTrigger,
}

impl From<ReplayEvent> for EmulationEvent {
    fn from(event: ReplayEvent) -> Self {
        match event {
            ReplayEvent::Joypad(state) => Self::Joypad(state),
            ReplayEvent::ZapperAim(pos) => Self::ZapperAim(pos),
            ReplayEvent::ZapperTrigger => Self::ZapperTrigger,
        }
    }
}

impl TryFrom<EmulationEvent> for ReplayEvent {
    type Error = anyhow::Error;

    fn try_from(event: EmulationEvent) -> Result<Self, Self::Error> {
        Ok(match event {
            EmulationEvent::Joypad(state) => Self::Joypad(state),
            EmulationEvent::ZapperAim(pos) => Self::ZapperAim(pos),
            EmulationEvent::ZapperTrigger => Self::ZapperTrigger,
            EmulationEvent::AddDebugger(_)
            | EmulationEvent::RemoveDebugger(_)
            | EmulationEvent::AudioRecord(_)
            | EmulationEvent::CpuCorrupted { .. }
            | EmulationEvent::DebugStep(_)
            | EmulationEvent::InstantRewind
            | EmulationEvent::LoadReplayPath(_)
            | EmulationEvent::LoadRom(_)
            | EmulationEvent::LoadRomPath(_)
            | EmulationEvent::LoadState(_)
            | EmulationEvent::RunState(_)
            | EmulationEvent::ReplayRecord(_)
            | EmulationEvent::Reset(_)
            | EmulationEvent::RequestFrame
            | EmulationEvent::Rewinding(_)
            | EmulationEvent::SaveState(_)
            | EmulationEvent::ShowFrameStats(_)
            | EmulationEvent::Screenshot
            | EmulationEvent::UnloadRom => {
                return Err(anyhow::anyhow!("invalid replay event: {event:?}"));
            }
            #[cfg(target_arch = "wasm32")]
            EmulationEvent::LoadReplay(_) => {
                return Err(anyhow::anyhow!("invalid replay event: {event:?}"));
            }
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[must_use]
pub(crate) struct ReplayFrame {
    pub(crate) frame: u32,
    pub(crate) event: ReplayEvent,
}

#[derive(Default, Debug)]
#[must_use]
pub(crate) struct Record {
    pub(crate) start: Option<Cpu>,
    pub(crate) events: Vec<ReplayFrame>,
}

impl Record {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn start(&mut self, cpu: Cpu) {
        self.start = Some(cpu);
        self.events.clear();
    }

    pub(crate) fn stop(&mut self, name: &str) -> anyhow::Result<Option<PathBuf>> {
        self.save(name)
    }

    pub(crate) fn push(&mut self, frame: u32, event: EmulationEvent) {
        if self.start.is_some()
            && let Ok(event) = ReplayEvent::try_from(event)
        {
            self.events.push(ReplayFrame { frame, event });
        }
    }

    /// Saves the replay recording out to a file.
    pub(crate) fn save(&mut self, name: &str) -> anyhow::Result<Option<PathBuf>> {
        let Some(start) = self.start.take() else {
            return Ok(None);
        };

        if self.events.is_empty() {
            tracing::debug!("not saving - no replay events");
            return Ok(None);
        }

        let replay_path = Config::default_data_dir()
            .join(
                Local::now()
                    .format(&format!("tetanes_replay_{name}_%Y-%m-%d_%H.%M.%S"))
                    .to_string(),
            )
            .with_extension("replay");
        let events = std::mem::take(&mut self.events);

        fs::save(&replay_path, &State((start, events)))?;

        Ok(Some(replay_path))
    }
}

#[derive(Default, Debug)]
#[must_use]
pub(crate) struct Replay {
    pub(crate) events: Vec<ReplayFrame>,
}

impl Replay {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Loads a replay recording file.
    pub(crate) fn load_path(&mut self, path: impl AsRef<Path>) -> anyhow::Result<Cpu> {
        let path = path.as_ref();
        let State((cpu, mut events)) = fs::load(path)?;
        events.reverse(); // So we can pop off the end
        self.events = events;
        Ok(cpu)
    }

    /// Loads a replay from a reader.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn load(&mut self, mut replay: impl std::io::Read) -> anyhow::Result<Cpu> {
        let mut events = Vec::new();
        replay.read_to_end(&mut events)?;
        let State((cpu, mut events)) = fs::load_bytes(&events)?;
        events.reverse(); // So we can pop off the end
        self.events = events;
        Ok(cpu)
    }

    pub(crate) fn next(&mut self, frame: u32) -> Option<EmulationEvent> {
        if let Some(event) = self.events.last() {
            match event.frame.cmp(&frame) {
                Ordering::Less | Ordering::Equal => {
                    if event.frame < frame {
                        warn!("out of order replay event: {} < {frame}", event.frame);
                    }
                    return self.events.pop().map(|event| event.event).map(Into::into);
                }
                Ordering::Greater => (),
            }
        }
        None
    }
}
