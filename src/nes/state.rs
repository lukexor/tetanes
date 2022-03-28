use crate::{
    common::config_dir,
    cpu::Cpu,
    nes::{
        filesystem::{validate_save_header, write_save_header},
        Nes,
    },
    NesResult,
};
use anyhow::{anyhow, Context};
use std::{
    ffi::OsStr,
    fs::{create_dir_all, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
};

impl Nes {
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
        let save = || -> NesResult<()> {
            let save_path = self.save_path()?;
            let save_dir = save_path.parent().unwrap(); // Safe to do because save_path is never root
            if !save_dir.exists() {
                create_dir_all(save_dir).with_context(|| {
                    anyhow!("failed to create save directory {:?}", save_dir.display())
                })?;
            }
            let save_file = File::create(&save_path)
                .with_context(|| anyhow!("failed to create save file {:?}", save_path.display()))?;
            let mut writer = BufWriter::new(save_file);
            write_save_header(&mut writer).with_context(|| {
                anyhow!("failed to write save header {:?}", save_path.display())
            })?;
            writer.write_all(&bincode::serialize(self.control_deck.cpu())?)?;
            Ok(())
        };
        match save() {
            Ok(_) => self.add_message(&format!("Saved Slot {}", slot)),
            Err(e) => self.add_message(&e.to_string()),
        }
    }

    /// Load the console with data saved from a save state
    pub(crate) fn load_state(&mut self) {
        let slot = self.config.save_slot;
        let mut load = || -> NesResult<()> {
            let save_path = self.save_path()?;
            let save_file = File::open(&save_path)
                .with_context(|| anyhow!("Failed to open file {:?}", save_path.display()))?;
            let mut reader = BufReader::new(save_file);
            validate_save_header(&mut reader)
                .and_then(|_| {
                    let mut bytes = vec![];
                    reader.read_to_end(&mut bytes)?;
                    bincode::deserialize::<Cpu>(&bytes)
                        .map(|cpu| self.control_deck.load_cpu(cpu))
                        .with_context(|| anyhow!("Failed to load save #{}", slot))
                })
                .with_context(|| anyhow!("Failed to load save #{}", slot))
        };
        match load() {
            Ok(()) => self.add_message(&format!("Loaded Slot {}", slot)),
            Err(e) => self.add_message(&e.to_string()),
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
            let sram_dir = sram_path.parent().unwrap(); // Safe to do because sram_path is never root
            if !sram_dir.exists() {
                create_dir_all(sram_dir).with_context(|| {
                    anyhow!("failed to create directory {:?}", sram_dir.display())
                })?;
            }

            let mut sram_opts = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&sram_path)
                .with_context(|| anyhow!("failed to open file {:?}", sram_path.display()))?;

            // Empty file means we just created it
            if sram_opts.metadata()?.len() == 0 {
                let mut sram_file = BufWriter::new(sram_opts);
                write_save_header(&mut sram_file)
                    .with_context(|| anyhow!("failed to write header {:?}", sram_path.display()))?;
                cart.save_sram(&mut sram_file)
            } else {
                // Check if exists and header is different, so we avoid overwriting
                validate_save_header(&mut sram_opts)
                    .and_then(|_| {
                        let mut sram_file = BufWriter::new(sram_opts);
                        cart.save_sram(&mut sram_file)
                    })
                    .with_context(|| anyhow!("failed to write sram due to invalid header",))
            }
        } else {
            Ok(())
        }
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    pub(super) fn load_sram(&mut self) -> NesResult<()> {
        let sram_path = self.sram_path()?;
        let cart = self.control_deck.cart_mut();
        if cart.battery_backed() && sram_path.exists() {
            let sram_file = File::open(&sram_path)
                .with_context(|| anyhow!("failed to open file {:?}", sram_path.display()))?;
            let mut sram_file = BufReader::new(sram_file);
            validate_save_header(&mut sram_file)
                .and_then(|_| {
                    cart.load_sram(&mut sram_file).with_context(|| {
                        anyhow!("failed to load save sram")
                    })
                })
                .with_context(|| anyhow!(
                    "failed to load sram. move or delete `{}` before exiting, otherwise sram data will be lost.",
                    sram_path.display()
                ))
        } else {
            Ok(())
        }
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
