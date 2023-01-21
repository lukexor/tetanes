use crate::{
    common::config_dir,
    cpu::Cpu,
    nes::{
        event::ActionEvent,
        filesystem::{decode_data, encode_data, load_data, save_data},
        menu::Menu,
        Mode, Nes,
    },
    NesError, NesResult,
};
use anyhow::{anyhow, Context};
use chrono::{DateTime, Local};
use pix_engine::prelude::{PixResult, PixState};
use serde::{Deserialize, Serialize};
use std::{ffi::OsStr, path::PathBuf};

/// Represents which mode the emulator is in for the Replay feature.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub(crate) enum ReplayMode {
    Off,
    Recording,
    Playback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub(crate) struct Replay {
    pub(crate) mode: ReplayMode,
    pub(crate) start: Option<Cpu>,
    pub(crate) buffer: Vec<ActionEvent>,
}

impl Default for Replay {
    fn default() -> Self {
        Self {
            mode: ReplayMode::Off,
            start: None,
            buffer: vec![],
        }
    }
}

impl Nes {
    pub(crate) fn handle_emulation_error(
        &mut self,
        s: &mut PixState,
        err: &NesError,
    ) -> PixResult<()> {
        self.error = Some(err.to_string());
        self.open_menu(s, Menu::LoadRom)
    }

    pub(crate) fn resume_play(&mut self) {
        if self.control_deck.is_running() {
            self.mode = Mode::Playing;
            self.audio.resume();
        }
    }

    pub(crate) fn pause_play(&mut self) {
        if self.control_deck.is_running() {
            self.mode = Mode::Paused;
            self.audio.pause();
        }
    }

    /// Returns the path where battery-backed Save RAM files are stored
    pub(crate) fn sram_path(&self) -> NesResult<PathBuf> {
        match self.control_deck.loaded_rom() {
            Some(ref rom) => PathBuf::from(rom)
                .file_stem()
                .and_then(OsStr::to_str)
                .map_or_else(
                    || Err(anyhow!("failed to create sram path for `{rom:?}`")),
                    |save_name| {
                        Ok(config_dir()
                            .join("sram")
                            .join(save_name)
                            .with_extension("sram"))
                    },
                ),
            None => Err(anyhow!("no rom is loaded")),
        }
    }

    /// Returns the path where Save states are stored
    pub(crate) fn save_path(&self, slot: u8) -> NesResult<PathBuf> {
        match self.control_deck.loaded_rom() {
            Some(ref rom) => PathBuf::from(rom)
                .file_stem()
                .and_then(OsStr::to_str)
                .map_or_else(
                    || Err(anyhow!("failed to create save path for `{rom:?}`")),
                    |save_name| {
                        Ok(config_dir()
                            .join("save")
                            .join(save_name)
                            .join(slot.to_string())
                            .with_extension("save"))
                    },
                ),
            None => Err(anyhow!("no rom is loaded")),
        }
    }

    /// Save the current state of the console into a save file
    pub(crate) fn save_state(&mut self, slot: u8) {
        // Avoid saving any test roms
        if self.config.rom_path.to_string_lossy().contains("test") {
            return;
        }
        match self.save_path(slot).and_then(|save_path| {
            bincode::serialize(self.control_deck.cpu())
                .context("failed to serialize save state")
                .map(|data| save_data(save_path, &data))
        }) {
            Ok(_) => self.add_message(format!("Saved slot {slot}")),
            Err(err) => {
                log::error!("{:?}", err);
                self.add_message(format!("Failed to save slot {slot}"));
            }
        }
    }

    /// Load the console with data saved from a save state
    pub(crate) fn load_state(&mut self, slot: u8) {
        match self.save_path(slot) {
            Ok(path) => {
                if path.exists() {
                    match load_data(path).and_then(|data| {
                        bincode::deserialize(&data)
                            .context("failed to deserialize load state")
                            .map(|cpu| self.control_deck.load_cpu(cpu))
                    }) {
                        Ok(_) => self.add_message(format!("Loaded slot {slot}")),
                        Err(err) => {
                            log::error!("{:?}", err);
                            self.add_message(format!("Failed to load slot {slot}"));
                        }
                    }
                } else {
                    self.add_message(format!("No save state found for slot {slot}"));
                }
            }
            Err(err) => {
                log::error!("{:?}", err);
                self.add_message(format!("Failed to determine save path {slot}"));
            }
        }
    }

    pub(crate) fn save_screenshot(&mut self, s: &mut PixState) {
        let filename = Local::now()
            .format("Screen_Shot_%Y-%m-%d_at_%H_%M_%S.png")
            .to_string();
        match s.save_canvas(None, &filename) {
            Ok(()) => self.add_message(filename),
            Err(err) => {
                log::error!("{err:?}");
                self.add_message("Failed to save screenshot");
            }
        }
    }

    pub(crate) fn update_rewind(&mut self) {
        if !self.config.rewind {
            return;
        }
        self.rewind_frame = self.rewind_frame.wrapping_add(1);
        if self.rewind_frame >= self.config.rewind_frames {
            self.rewind_frame = 0;
            if let Err(err) = bincode::serialize(self.control_deck.cpu())
                .context("failed to serialize rewind state")
                .and_then(|data| encode_data(&data))
                .map(|data| self.rewind_buffer.push_front(data))
            {
                log::error!("{err:?}");
                self.config.rewind = false;
                self.rewind_buffer.clear();
                return;
            }
            let buffer_size = self
                .rewind_buffer
                .iter()
                .fold(0, |size, data| size + data.len());
            if buffer_size > self.config.rewind_buffer_size * 1024 * 1024 {
                self.rewind_buffer.truncate(self.rewind_buffer.len() / 2);
            }
        }
    }

    pub(crate) fn rewind(&mut self) {
        if let Some(data) = self.rewind_buffer.pop_front() {
            if let Err(err) = decode_data(&data).and_then(|data| {
                bincode::deserialize(&data)
                    .context("failed to deserialize rewind state")
                    .map(|cpu| self.control_deck.load_cpu(cpu))
            }) {
                log::error!("{err:?}");
                self.config.rewind = false;
                self.rewind_buffer.clear();
            }
        }
    }

    pub(crate) fn instant_rewind(&mut self) {
        if self.config.rewind {
            // Two seconds worth of frames @ 60 FPS
            let mut rewind_frames = 120 / self.config.rewind_frames as usize;
            while rewind_frames > 0 {
                self.rewind_buffer.pop_front();
                rewind_frames -= 1;
            }

            if let Some(data) = self.rewind_buffer.pop_front() {
                self.add_message("Rewind");
                if let Err(err) = decode_data(&data).and_then(|data| {
                    bincode::deserialize(&data)
                        .context("failed to deserialize rewind state")
                        .map(|cpu| self.control_deck.load_cpu(cpu))
                }) {
                    log::error!("{err:?}");
                    self.config.rewind = false;
                    self.rewind_buffer.clear();
                }
            }
        } else {
            self.add_message("Rewind disabled. You can enable it in the Config menu.");
        }
    }

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    pub(crate) fn save_sram(&self) -> NesResult<()> {
        if self.control_deck.cart_battery_backed() {
            let sram_path = self.sram_path()?;
            save_data(sram_path, self.control_deck.sram())?;
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    pub(crate) fn load_sram(&mut self) -> NesResult<()> {
        let sram_path = self.sram_path()?;
        if self.control_deck.cart_battery_backed() && sram_path.exists() {
            load_data(&sram_path).map(|data| self.control_deck.load_sram(data))?;
        }
        Ok(())
    }

    pub(crate) fn start_replay(&mut self) {
        self.replay.start = Some(self.control_deck.cpu().clone());
        self.replay.mode = ReplayMode::Recording;
        self.add_message("Replay Recording Started");
    }

    pub(crate) fn stop_replay(&mut self) {
        if self.replay.mode == ReplayMode::Playback {
            self.add_message("Replay Playback Stopped");
        } else {
            self.add_message("Replay Recording Stopped");
            self.save_replay();
        }
        self.replay.mode = ReplayMode::Off;
    }

    /// Saves the replay buffer out to a file
    pub(crate) fn save_replay(&mut self) {
        let datetime: DateTime<Local> = Local::now();
        let replay_path =
            PathBuf::from(datetime.format("tetanes_%Y-%m-%d_at_%H.%M.%S").to_string())
                .with_extension("replay");
        self.replay.buffer.reverse();
        match bincode::serialize(&self.replay)
            .context("failed to serialize replay recording")
            .map(|data| save_data(replay_path, &data))
        {
            Ok(_) => {
                self.replay.buffer.clear();
                self.add_message("Saved replay recording");
            }
            Err(err) => {
                log::error!("{err:?}");
                self.add_message("Failed to save replay recording");
            }
        }
    }

    /// Loads a replay file
    pub(crate) fn load_replay(&mut self) {
        if let Some(replay_path) = &self.replay_path {
            match load_data(replay_path).and_then(|data| {
                bincode::deserialize::<Replay>(&data)
                    .context("failed to deserialize replay recording")
                    .map(|mut replay| {
                        self.control_deck
                            .load_cpu(replay.start.take().expect("valid replay start"));
                        self.replay = replay;
                        self.replay.mode = ReplayMode::Playback;
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

    pub(crate) fn toggle_pause(&mut self, s: &mut PixState) -> NesResult<()> {
        match self.mode {
            Mode::Playing | Mode::Rewinding => {
                self.mode = Mode::Paused;
            }
            Mode::Paused | Mode::PausedBg => {
                self.resume_play();
            }
            Mode::InMenu(..) => self.exit_menu(s)?,
        }
        Ok(())
    }

    pub(crate) fn toggle_sound_recording(&mut self, _s: &mut PixState) {
        self.record_sound = !self.record_sound;
        // TODO
        self.add_message("Toggle sound recording not implemented yet");
    }
}
