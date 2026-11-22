use crate::nes::{config::Config, event::EmulationEvent};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    io::Read,
    path::{Path, PathBuf},
};
use tetanes_core::{
    bus::Bus,
    fs,
    input::{JoypadBtn, Player},
};
use tracing::warn;
use winit::event::ElementState;

#[derive(Debug, Serialize, Deserialize)]
pub struct State((Bus, Vec<ReplayFrame>));

#[derive(Debug, Serialize, Deserialize)]
pub enum ReplayEvent {
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
            _ => return Err(anyhow::anyhow!("invalid replay event: {event:?}")),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[must_use]
pub struct ReplayFrame {
    pub frame: u32,
    pub event: ReplayEvent,
}

#[derive(Default, Debug)]
#[must_use]
pub struct Record {
    pub start: Option<Bus>,
    pub events: Vec<ReplayFrame>,
}

impl Record {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, bus: Bus) {
        self.start = Some(bus);
        self.events.clear();
    }

    pub fn stop(&mut self, name: &str) -> anyhow::Result<Option<PathBuf>> {
        self.save(name)
    }

    pub fn push(&mut self, frame: u32, event: EmulationEvent) {
        if self.start.is_some()
            && let Ok(event) = ReplayEvent::try_from(event)
        {
            self.events.push(ReplayFrame { frame, event });
        }
    }

    /// Saves the replay recording out to a file.
    pub fn save(&mut self, name: &str) -> anyhow::Result<Option<PathBuf>> {
        let Some(start) = self.start.take() else {
            return Ok(None);
        };

        if self.events.is_empty() {
            tracing::debug!("not saving - no replay events");
            return Ok(None);
        }

        let replay_path = Config::default_data_dir()
            .join(
                jiff::ZonedDateTime::now()
                    .strftime(&format!("tetanes_replay_{name}_%Y-%m-%d_%H.%M.%S"))
                    .to_string(),
            )
            .with_extension("replay");
        let events = std::mem::take(&mut self.events);

        fs::save_path(&replay_path, &State((start, events)))?;

        Ok(Some(replay_path))
    }
}

#[derive(Default, Debug)]
#[must_use]
pub struct Replay {
    pub events: Vec<ReplayFrame>,
}

impl Replay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads a replay recording file.
    pub fn load_path(&mut self, path: impl AsRef<Path>) -> anyhow::Result<Bus> {
        self.load(fs::load_raw_path(path)?.as_slice())
    }

    /// Loads a replay from a reader.
    pub fn load(&mut self, replay: impl Read) -> anyhow::Result<Bus> {
        let (version, data) = fs::load_data(replay)?;
        let State((bus, mut events)) = if version == fs::v3::SAVE_VERSION {
            // The console, then the events, each decoded with its current type.
            let (bus, len) = fs::v3::decode_state(&data)?;
            let (events, _) = fs::v3::decode(&data[len..])?;
            State((bus, events))
        } else {
            fs::check_version(version, fs::SAVE_VERSION)?;
            fs::decode_value(&data)?
        };
        events.reverse(); // So we can pop off the end
        self.events = events;
        Ok(bus)
    }

    pub fn next(&mut self, frame: u32) -> Option<EmulationEvent> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A replay reads back its events in the order they happened.
    #[test]
    fn a_replay_round_trips() {
        let events = vec![
            ReplayFrame {
                frame: 1,
                event: ReplayEvent::ZapperTrigger,
            },
            ReplayFrame {
                frame: 2,
                event: ReplayEvent::Joypad((Player::One, JoypadBtn::A, ElementState::Pressed)),
            },
        ];
        let mut file = Vec::new();
        fs::save(&mut file, &State((Bus::default(), events))).expect("saves");

        let mut replay = Replay::new();
        let _ = replay.load(file.as_slice()).expect("loads");
        assert!(matches!(
            replay.next(1),
            Some(EmulationEvent::ZapperTrigger)
        ));
        assert!(matches!(replay.next(2), Some(EmulationEvent::Joypad(_))));
    }

    /// A replay recorded by 0.18, whose console and events are bincode.
    #[test]
    fn a_version_3_replay_loads() {
        const V3: &[u8] =
            include_bytes!("../../../../tetanes-core/test_roms/save_states/v3/spritecans.replay");
        const ROM: &[u8] = include_bytes!("../../../../tetanes-core/test_roms/spritecans.nes");
        let mut replay = Replay::new();
        let bus = replay.load(V3).expect("loads");
        assert!(replay.events.is_empty());

        let mut deck = tetanes_core::control_deck::ControlDeck::new();
        deck.load_rom("spritecans", &mut &ROM[..])
            .expect("rom loads");
        deck.load_bus(bus)
            .expect("the console is the recorded cart's");
    }
}
