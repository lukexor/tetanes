use crate::{
    common::config_dir,
    nes::{
        filesystem::{load_data, save_data},
        Mode, Nes,
    },
    NesResult,
};
use anyhow::{anyhow, Context};

use std::{ffi::OsStr, path::PathBuf};

impl Nes {
    pub(crate) fn resume_play(&mut self) {
        if self.control_deck.is_running() {
            self.mode = Mode::Playing;
        }
    }

    pub(crate) fn pause_play(&mut self) {
        if self.control_deck.is_running() {
            self.mode = Mode::Paused;
        }
    }

    /// Returns the path where battery-backed Save RAM files are stored
    pub(crate) fn sram_path(&self) -> NesResult<PathBuf> {
        match self.control_deck.loaded_rom() {
            Some(ref rom) => PathBuf::from(rom)
                .file_stem()
                .and_then(OsStr::to_str)
                .map_or_else(
                    || Err(anyhow!("failed to create sram path for `{}`", rom)),
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
    pub(crate) fn save_path(&self) -> NesResult<PathBuf> {
        match self.control_deck.loaded_rom() {
            Some(ref rom) => PathBuf::from(rom)
                .file_stem()
                .and_then(OsStr::to_str)
                .map_or_else(
                    || Err(anyhow!("failed to create save path for `{}`", rom)),
                    |save_name| {
                        Ok(config_dir()
                            .join("save")
                            .join(save_name)
                            .join(self.config.save_slot.to_string())
                            .with_extension("save"))
                    },
                ),
            None => Err(anyhow!("no rom is loaded")),
        }
    }
    /// Save the current state of the console into a save file
    pub(crate) fn save_state(&mut self) {
        let slot = self.config.save_slot;
        match self.save_path().and_then(|save_path| {
            bincode::serialize(self.control_deck.cpu())
                .context("failed to serialize save state")
                .map(|data| save_data(save_path, &data))
        }) {
            Ok(_) => self.add_message(format!("Saved slot {}", slot)),
            Err(err) => {
                log::error!("{:?}", err);
                self.add_message(format!("Failed to save slot {}", slot));
            }
        }
    }

    /// Load the console with data saved from a save state
    pub(crate) fn load_state(&mut self) {
        let slot = self.config.save_slot;
        match self.save_path().and_then(load_data).and_then(|data| {
            bincode::deserialize(&data)
                .context("failed to deserialize load state")
                .map(|cpu| self.control_deck.load_cpu(cpu))
        }) {
            Ok(_) => self.add_message(format!("Loaded slot {}", slot)),
            Err(err) => {
                log::error!("{:?}", err);
                self.add_message(format!("Failed to load slot {}", slot));
            }
        }
    }

    // pub(super) fn save_rewind(&mut self, elapsed: f64) {
    //     if self.config.rewind_enabled {
    //         self.rewind_timer -= elapsed;
    //         if self.rewind_timer <= 0.0 {
    //             self.rewind_timer = REWIND_TIMER;
    //             let rewind_slot = if self.rewind_queue.len() >= REWIND_SIZE as usize {
    //                 self.rewind_queue.pop_front().unwrap() // Safe to unwrap
    //             } else {
    //                 REWIND_SLOT + self.rewind_queue.len() as u8
    //             };
    //             let rewind = true;
    //             self.save_state(rewind_slot, rewind);
    //             self.rewind_queue.push_back(rewind_slot);
    //         }
    //     }
    // }

    // pub(super) fn rewind(&mut self) {
    //     if self.config.rewind_enabled {
    //         if let Some(rewind_slot) = self.rewind_queue.pop_back() {
    //             self.add_message("Rewind");
    //             let rewind = true;
    //             self.load_state(rewind_slot, rewind);
    //         }
    //     } else {
    //         self.add_message("Rewind disabled");
    //     }
    // }

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    pub(super) fn save_sram(&mut self) -> NesResult<()> {
        let cart = &self.control_deck.cart();
        if cart.battery_backed() {
            let sram_path = self.sram_path()?;
            save_data(sram_path, cart.sram())?;
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    pub(super) fn load_sram(&mut self) -> NesResult<()> {
        let sram_path = self.sram_path()?;
        let cart = self.control_deck.cart_mut();
        if cart.battery_backed() && sram_path.exists() {
            load_data(&sram_path).map(|data| cart.load_sram(data))?;
        }
        Ok(())
    }

    // /// Saves the replay buffer out to a file
    // pub fn save_replay(&mut self) -> NesResult<()> {
    //     let datetime: DateTime<Local> = Local::now();
    //     let mut path = PathBuf::from(datetime.format("tetanes_%Y-%m-%d_at_%H.%M.%S").to_string());
    //     path.set_extension("replay");
    //     let file = File::create(&path)?;
    //     let mut file = BufWriter::new(file);
    //     self.replay_buffer.save(&mut file)?;
    //     println!("Saved replay: {:?}", path);
    //     Ok(())
    // }

    // /// Loads a replay file into a Vec
    // pub(super) fn load_replay(&self) -> NesResult<Vec<FrameEvent>> {
    //     if let Some(replay) = &self.config.replay {
    //         let file = File::open(&PathBuf::from(replay))
    //             .map_err(|e| map_nes_err!("failed to open file {:?}: {}", replay, e))?;
    //         let mut file = BufReader::new(file);
    //         let mut buffer: Vec<FrameEvent> = Vec::new();
    //         buffer.load(&mut file)?;
    //         buffer.reverse();
    //         Ok(buffer)
    //     } else {
    //         Ok(Vec::new())
    //     }
    // }
}
