use crate::{
    common::{home_dir, Clocked, Powered, CONFIG_DIR},
    map_nes_err, mapper,
    mapper::Mapper,
    nes::{debug::DEBUG_WIDTH, event::FrameEvent, Nes, REWIND_SIZE, REWIND_SLOT, REWIND_TIMER},
    nes_err,
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    serialization::{validate_save_header, write_save_header, Savable},
    NesResult,
};
use chrono::prelude::{DateTime, Local};
use log::error;
use pix_engine::prelude::*;
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

impl Nes {
    pub(super) fn create_textures(&mut self, s: &mut PixState) -> NesResult<()> {
        self.screen = s.create_texture(PixelFormat::Rgb, RENDER_WIDTH, RENDER_HEIGHT)?;
        // s.create_texture(
        //     "message",
        //     ColorType::Rgba,
        //     rect!(0, 0, self.width, MSG_HEIGHT),
        //     rect!(0, 0, self.width, MSG_HEIGHT),
        // )?;
        // s.create_texture(
        //     "menu",
        //     ColorType::Rgba,
        //     rect!(0, 0, self.width, self.height),
        //     rect!(0, 0, self.width, self.height),
        // )?;
        // s.create_texture(
        //     "debug",
        //     ColorType::Rgba,
        //     rect!(0, 0, DEBUG_WIDTH, self.height),
        //     rect!(self.width, 0, DEBUG_WIDTH, self.height),
        // )?;
        Ok(())
    }

    pub(super) fn paused(&mut self, paused: bool) {
        if !self.paused && paused {
            self.set_static_message("Paused");
        } else if !paused {
            self.unset_static_message("Paused");
        }
        self.paused = paused;
    }

    /// Changes the savestate slot
    pub(super) fn set_save_slot(&mut self, slot: u8) {
        if self.config.save_enabled {
            if self.config.save_slot != slot {
                self.config.save_slot = slot;
                self.add_message(&format!("Set Save Slot to {}", slot));
            }
        } else {
            self.add_message("Savestates Disabled");
        }
    }

    /// Save the current state of the console into a save file
    pub(super) fn save_state(&mut self, slot: u8, rewind: bool) {
        if self.config.save_enabled || (rewind && self.config.rewind_enabled) {
            let save = || -> NesResult<()> {
                let save_path = save_path(&self.loaded_rom, slot)?;
                let save_dir = save_path.parent().unwrap(); // Safe to do because save_path is never root
                if !save_dir.exists() {
                    std::fs::create_dir_all(save_dir).map_err(|e| {
                        map_nes_err!("failed to create directory {:?}: {}", save_dir.display(), e)
                    })?;
                }
                let save_file = std::fs::File::create(&save_path).map_err(|e| {
                    map_nes_err!("failed to create file {:?}: {}", save_path.display(), e)
                })?;
                let mut writer = BufWriter::new(save_file);
                write_save_header(&mut writer).map_err(|e| {
                    map_nes_err!("failed to write header {:?}: {}", save_path.display(), e)
                })?;
                self.save(&mut writer)?;
                Ok(())
            };
            let save = save();
            if !rewind {
                match save {
                    Ok(_) => self.add_message(&format!("Saved Slot {}", slot)),
                    Err(e) => self.add_message(&e.to_string()),
                }
            } else if let Err(e) = save {
                eprintln!("{}", &e.to_string());
            }
        } else {
            self.add_message("Savestates Disabled");
        }
    }

    /// Load the console with data saved from a save state
    pub(super) fn load_state(&mut self, slot: u8, rewind: bool) {
        if self.config.save_enabled || (rewind && self.config.rewind_enabled) {
            if let Ok(save_path) = save_path(&self.loaded_rom, slot) {
                if save_path.exists() {
                    let mut load = || -> NesResult<()> {
                        let save_file = std::fs::File::open(&save_path).map_err(|e| {
                            map_nes_err!("Failed to open file {:?}: {}", save_path.display(), e)
                        })?;
                        let mut reader = BufReader::new(save_file);
                        match validate_save_header(&mut reader) {
                            Ok(_) => {
                                if let Err(e) = self.load(&mut reader) {
                                    self.power_cycle();
                                    return nes_err!("Failed to load savestate #{}: {}", slot, e);
                                }
                            }
                            Err(e) => return nes_err!("Failed to load savestate #{}: {}", slot, e),
                        }
                        Ok(())
                    };
                    let load = load();
                    if !rewind {
                        match load {
                            Ok(()) => self.add_message(&format!("Loaded Slot {}", slot)),
                            Err(e) => self.add_message(&e.to_string()),
                        }
                    } else if let Err(e) = load {
                        eprintln!("{}", &e.to_string());
                    }
                }
            }
        } else {
            self.add_message("Savestates Disabled");
        }
    }

    pub(super) fn save_rewind(&mut self, elapsed: f64) {
        if self.config.rewind_enabled {
            self.rewind_timer -= elapsed;
            if self.rewind_timer <= 0.0 {
                self.rewind_timer = REWIND_TIMER;
                let rewind_slot = if self.rewind_queue.len() >= REWIND_SIZE as usize {
                    self.rewind_queue.pop_front().unwrap() // Safe to unwrap
                } else {
                    REWIND_SLOT + self.rewind_queue.len() as u8
                };
                let rewind = true;
                self.save_state(rewind_slot, rewind);
                self.rewind_queue.push_back(rewind_slot);
            }
        }
    }

    pub(super) fn rewind(&mut self) {
        if self.config.rewind_enabled {
            if let Some(rewind_slot) = self.rewind_queue.pop_back() {
                self.add_message("Rewind");
                let rewind = true;
                self.load_state(rewind_slot, rewind);
            }
        } else {
            self.add_message("Rewind disabled");
        }
    }

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    pub(super) fn save_sram(&mut self) -> NesResult<()> {
        let mapper = &self.cpu.bus.mapper;
        if mapper.battery_backed() {
            let sram_path = sram_path(&self.loaded_rom)?;
            let sram_dir = sram_path.parent().unwrap(); // Safe to do because sram_path is never root
            if !sram_dir.exists() {
                std::fs::create_dir_all(sram_dir).map_err(|e| {
                    map_nes_err!("failed to create directory {:?}: {}", sram_dir.display(), e)
                })?;
            }

            let mut sram_opts = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&sram_path)
                .map_err(|e| {
                    map_nes_err!("failed to open file {:?}: {}", sram_path.display(), e)
                })?;

            // Empty file means we just created it
            if sram_opts.metadata()?.len() == 0 {
                let mut sram_file = BufWriter::new(sram_opts);
                write_save_header(&mut sram_file).map_err(|e| {
                    map_nes_err!("failed to write header {:?}: {}", sram_path.display(), e)
                })?;
                mapper.save_sram(&mut sram_file)?;
            } else {
                // Check if exists and header is different, so we avoid overwriting
                match validate_save_header(&mut sram_opts) {
                    Ok(_) => {
                        let mut sram_file = BufWriter::new(sram_opts);
                        mapper.save_sram(&mut sram_file)?;
                    }
                    Err(e) => {
                        return nes_err!("failed to write sram due to invalid header. error: {}", e)
                    }
                }
            }
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    pub(super) fn load_sram(&mut self) -> NesResult<()> {
        let mapper = &mut self.cpu.bus.mapper;
        if mapper.battery_backed() {
            let sram_path = sram_path(&self.loaded_rom)?;
            if sram_path.exists() {
                let sram_file = std::fs::File::open(&sram_path).map_err(|e| {
                    map_nes_err!("failed to open file {:?}: {}", sram_path.display(), e)
                })?;
                let mut sram_file = BufReader::new(sram_file);
                match validate_save_header(&mut sram_file) {
                            Ok(_) => {
                                if let Err(e) = mapper.load_sram(&mut sram_file) {
                                    return nes_err!("failed to load save sram: {}", e);
                                }
                            }
                            Err(e) => return nes_err!(
                                "failed to load sram: {}.\n  move or delete `{}` before exiting, otherwise sram data will be lost.",
                                e,
                                sram_path.display()
                            ),
                        }
            }
        }
        Ok(())
    }

    /// Saves the replay buffer out to a file
    pub fn save_replay(&mut self) -> NesResult<()> {
        let datetime: DateTime<Local> = Local::now();
        let mut path = PathBuf::from(datetime.format("tetanes_%Y-%m-%d_at_%H.%M.%S").to_string());
        path.set_extension("replay");
        let file = std::fs::File::create(&path)?;
        let mut file = BufWriter::new(file);
        self.replay_buffer.save(&mut file)?;
        println!("Saved replay: {:?}", path);
        Ok(())
    }

    /// Loads a replay file into a Vec
    pub(super) fn load_replay(&self) -> NesResult<Vec<FrameEvent>> {
        if let Some(replay) = &self.config.replay {
            let file = std::fs::File::open(&PathBuf::from(replay))
                .map_err(|e| map_nes_err!("failed to open file {:?}: {}", replay, e))?;
            let mut file = BufReader::new(file);
            let mut buffer: Vec<FrameEvent> = Vec::new();
            buffer.load(&mut file)?;
            buffer.reverse();
            Ok(buffer)
        } else {
            Ok(Vec::new())
        }
    }

    pub(super) fn check_window_focus(&mut self) {
        if self.config.pause_in_bg {
            if self.focused_window.is_none() {
                // Only pause and set background_pause if we weren't already paused
                if !self.paused && self.config.pause_in_bg {
                    self.background_pause = true;
                }
                self.paused(true);
            } else if self.background_pause {
                self.background_pause = false;
                // Only unpause if we weren't paused as a result of losing focus
                self.paused(false);
            }
        }
    }
}

/// Returns the path where battery-backed Save RAM files are stored
///
/// # Arguments
///
/// * `path` - An object that implements AsRef<Path> that holds the path to the currently
/// running ROM
///
/// # Errors
///
/// Panics if path is not a valid path
fn sram_path<P: AsRef<Path>>(path: &P) -> NesResult<PathBuf> {
    let save_name = path.as_ref().file_stem().and_then(|s| s.to_str()).unwrap();
    let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
    path.push(CONFIG_DIR);
    path.push("sram");
    path.push(save_name);
    path.set_extension("sram");
    Ok(path)
}

/// Returns the path where Save states are stored
///
/// # Arguments
///
/// * `path` - An object that implements AsRef<Path> that holds the path to the currently
/// running ROM
///
/// # Errors
///
/// Panics if path is not a valid path
pub fn save_path<P: AsRef<Path>>(path: &P, slot: u8) -> NesResult<PathBuf> {
    if let Some(save_name) = path.as_ref().file_stem().and_then(|s| s.to_str()) {
        let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
        path.push(CONFIG_DIR);
        path.push("save");
        path.push(save_name);
        path.push(format!("{}", slot));
        path.set_extension("save");
        Ok(path)
    } else {
        nes_err!("failed to create save path for {:?}", path.as_ref())
    }
}
