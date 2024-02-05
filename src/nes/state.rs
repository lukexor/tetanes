use crate::{
    cpu::Cpu,
    nes::{
        event::ActionEvent,
        filesystem::{decode_data, encode_data, load_data, save_data},
        menu::Menu,
        Nes,
    },
    NesError, NesResult,
};
use anyhow::{anyhow, Context};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[must_use]
pub enum PauseMode {
    Manual,
    Unfocused,
}

/// Represents which mode the emulator is in.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Mode {
    Playing { recording_audio: bool },
    Replay(ReplayMode),
    Rewinding,
    Paused(PauseMode),
    InMenu(Menu),
}

impl Default for Mode {
    fn default() -> Self {
        Self::InMenu(Menu::default())
    }
}

impl Mode {
    #[inline]
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        matches!(self, Self::Playing { .. })
    }

    #[inline]
    #[must_use]
    pub const fn is_rewinding(&self) -> bool {
        matches!(self, Self::Rewinding)
    }

    #[inline]
    #[must_use]
    pub const fn is_playback(&self) -> bool {
        matches!(self, Self::Replay(ReplayMode::Playback))
    }

    #[inline]
    #[must_use]
    pub const fn is_recording_playback(&self) -> bool {
        matches!(self, Self::Replay(ReplayMode::Recording))
    }

    #[inline]
    #[must_use]
    pub const fn is_recording_audio(&self) -> bool {
        matches!(
            self,
            Self::Playing {
                recording_audio: true,
                ..
            }
        )
    }

    #[inline]
    #[must_use]
    pub const fn is_paused(&self) -> bool {
        matches!(self, Self::Paused(..))
    }

    #[inline]
    #[must_use]
    pub const fn is_paused_unfocused(&self) -> bool {
        matches!(self, Self::Paused(PauseMode::Unfocused))
    }

    #[inline]
    #[must_use]
    pub const fn is_in_menu(&self) -> bool {
        matches!(self, Self::InMenu(..))
    }
}

/// Represents which mode the emulator is in for the Replay feature.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ReplayMode {
    Off,
    Recording,
    Playback,
}

impl ReplayMode {
    #[inline]
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        matches!(self, Self::Recording)
    }

    #[inline]
    #[must_use]
    pub const fn is_playback(&self) -> bool {
        matches!(self, Self::Playback)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Replay {
    pub mode: ReplayMode,
    pub start: Option<Cpu>,
    pub buffer: Vec<ActionEvent>,
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
    pub fn handle_emulation_error(&mut self, err: &NesError) {
        self.error = Some(err.to_string());
        self.mode = Mode::Paused(PauseMode::Manual);
    }

    pub fn resume_play(&mut self) {
        self.mode = Mode::Playing {
            recording_audio: false,
        };
        if self.control_deck.is_running() {
            self.renderer.resume();
            if let Err(err) = self.audio.play() {
                self.add_message(format!("failed to start audio: {err:?}"));
            }
        }
    }

    pub fn pause_play(&mut self, mode: PauseMode) {
        self.mode = Mode::Paused(mode);
        if self.control_deck.is_running() {
            self.renderer.pause();
            if self.mode.is_recording_playback() {
                self.stop_replay();
            }
            if self.mode.is_recording_audio() {
                self.audio.stop_recording();
            }
            self.audio.pause();
        }
    }

    /// Returns the path where battery-backed Save RAM files are stored
    pub(crate) fn sram_path(&self) -> NesResult<PathBuf> {
        match self.control_deck.loaded_rom() {
            Some(ref rom) => PathBuf::from(rom)
                .file_stem()
                .and_then(std::ffi::OsStr::to_str)
                .map_or_else(
                    || Err(anyhow!("failed to create sram path for `{rom:?}`")),
                    |save_name| {
                        Ok(super::config::Config::directory()
                            .join("sram")
                            .join(save_name)
                            .with_extension("sram"))
                    },
                ),
            None => Err(anyhow!("no rom is loaded")),
        }
    }

    /// Returns the path where Save states are stored
    pub fn save_path(&self, slot: u8) -> NesResult<PathBuf> {
        match self.control_deck.loaded_rom() {
            Some(ref rom) => PathBuf::from(rom)
                .file_stem()
                .and_then(std::ffi::OsStr::to_str)
                .map_or_else(
                    || Err(anyhow!("failed to create save path for `{rom:?}`")),
                    |save_name| {
                        Ok(super::config::Config::directory()
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
    #[cfg(target_arch = "wasm32")]
    pub fn save_state(&mut self, _slot: u8) {
        // TODO: save to local storage or indexdb
    }

    /// Save the current state of the console into a save file
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_state(&mut self, slot: u8) {
        use crate::common::{Kind, Reset};

        // Avoid saving any test roms
        if self.config.rom_path.to_string_lossy().contains("test") {
            return;
        }
        let mut cpu = self.control_deck.cpu().clone();
        cpu.input_mut().reset(Kind::Hard);
        match self.save_path(slot).and_then(|save_path| {
            bincode::serialize(&cpu)
                .context("failed to serialize save state")
                .map(|data| save_data(save_path, &data))
        }) {
            Ok(_) => self.add_message(format!("Saved state: Slot {slot}")),
            Err(err) => {
                log::error!("{:?}", err);
                self.add_message(format!("Failed to save slot {slot}"));
            }
        }
    }

    /// Load the console with data saved from a save state
    #[cfg(target_arch = "wasm32")]
    pub fn load_state(&mut self, _slot: u8) {
        // TODO: load from local storage or indexdb
    }

    /// Load the console with data saved from a save state
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_state(&mut self, slot: u8) {
        match self.save_path(slot) {
            Ok(path) => {
                if path.exists() {
                    match load_data(path).and_then(|data| {
                        bincode::deserialize(&data)
                            .context("failed to deserialize load state")
                            .map(|cpu| self.control_deck.load_cpu(cpu))
                    }) {
                        Ok(_) => self.add_message(format!("Loaded state: Slot {slot}")),
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

    pub fn save_screenshot(&mut self) {
        // TODO: Provide download file for WASM
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::ppu::Ppu;
            let filename = PathBuf::from(
                Local::now()
                    .format("screenshot_%Y-%m-%d_at_%H_%M_%S")
                    .to_string(),
            )
            .with_extension("png");
            let image = image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
                Ppu::WIDTH,
                Ppu::HEIGHT,
                self.control_deck.frame_buffer(),
            )
            .expect("valid frame buffer");

            match image.save(&filename) {
                Ok(()) => self.add_message(filename.to_string_lossy()),
                Err(err) => {
                    log::error!("{err:?}");
                    self.add_message("Failed to save screenshot");
                }
            }
        }
    }

    pub fn update_rewind(&mut self) {
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

    pub fn rewind(&mut self) {
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

    pub fn instant_rewind(&mut self) {
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
    pub fn save_sram(&self) -> NesResult<()> {
        if self.control_deck.cart_battery_backed() {
            log::info!("saving SRAM...");
            let sram_path = self.sram_path()?;
            save_data(sram_path, self.control_deck.sram())?;
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    pub fn load_sram(&mut self) -> NesResult<()> {
        if self.control_deck.cart_battery_backed() {
            log::info!("loading SRAM...");
            let sram_path = self.sram_path()?;
            if sram_path.exists() {
                load_data(&sram_path).map(|data| self.control_deck.load_sram(data))?;
            }
        }
        Ok(())
    }

    pub fn start_replay(&mut self) {
        self.replay.start = Some(self.control_deck.cpu().clone());
        self.replay.mode = ReplayMode::Recording;
        self.add_message("Replay Recording Started");
    }

    pub fn stop_replay(&mut self) {
        if self.replay.mode == ReplayMode::Playback {
            self.add_message("Replay Playback Stopped");
        } else {
            self.add_message("Replay Recording Stopped");
            self.save_replay();
        }
        self.replay.mode = ReplayMode::Off;
    }

    /// Saves the replay buffer out to a file
    pub fn save_replay(&mut self) {
        let replay_path = PathBuf::from(
            Local::now()
                .format("tetanes_%Y-%m-%d_at_%H.%M.%S")
                .to_string(),
        )
        .with_extension("replay");
        log::info!("saving replay to {replay_path:?}...",);
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
    pub fn load_replay(&mut self) {
        if let Some(replay_path) = &self.config.replay_path {
            log::info!("loading replay {replay_path:?}...",);
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

    pub fn toggle_pause(&mut self) {
        if self.mode.is_playing() {
            self.pause_play(PauseMode::Manual);
        } else if self.mode.is_paused() {
            self.resume_play();
        } else if self.mode.is_in_menu() {
            self.exit_menu();
        }
    }

    pub fn toggle_sound_recording(&mut self) {
        if self.mode.is_playing() {
            if !self.mode.is_recording_audio() {
                match self.audio.start_recording() {
                    Ok(_) => {
                        self.mode = Mode::Playing {
                            recording_audio: true,
                        };
                        self.add_message("Recording audio...");
                    }
                    Err(err) => {
                        log::error!("{err:?}");
                        self.add_message("Failed to start recording audio");
                    }
                }
            } else {
                self.audio.stop_recording();
                self.mode = Mode::Playing {
                    recording_audio: false,
                };
                self.add_message("Recording audio stopped.");
            }
        }
    }
}
