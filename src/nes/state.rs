use crate::{
    common::{home_dir, Clocked, Powered, CONFIG_DIR},
    logging::{LogLevel, Loggable},
    map_nes_err, mapper,
    nes::Nes,
    nes_err,
    serialization::{validate_save_header, write_save_header, Savable},
    NesResult,
};
use pix_engine::event::PixEvent;
use std::{
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

impl Nes {
    /// Powers on the console
    pub(super) fn power_on(&mut self) -> NesResult<()> {
        self.cpu.power_on();
        if let Err(e) = self.load_sram() {
            self.add_message(&e.to_string());
        }
        self.paused = false;
        self.cycles_remaining = 0.0;
        Ok(())
    }

    /// Powers off the console
    pub(super) fn power_off(&mut self) -> NesResult<()> {
        if self.recording {
            self.save_replay()?;
        }
        if let Err(e) = self.save_sram() {
            self.add_message(&e.to_string());
        }
        self.power_cycle();
        self.paused = true;
        Ok(())
    }

    /// Loads a ROM cartridge into memory
    pub(super) fn load_rom(&mut self, rom_id: usize) -> NesResult<()> {
        self.loaded_rom = self.roms[rom_id].to_owned();
        let mapper = mapper::load_rom(&self.loaded_rom)?;
        self.cpu.bus.load_mapper(mapper);
        Ok(())
    }

    /// Save the current state of the console into a save file
    pub(super) fn save_state(&mut self, slot: u8, rewind: bool) {
        if self.config.save_enabled {
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
            if !rewind {
                match save() {
                    Ok(_) => self.add_message(&format!("Saved Slot {}", slot)),
                    Err(e) => self.add_message(&e.to_string()),
                }
            }
        } else {
            self.add_message("Savestates Disabled");
        }
    }

    /// Load the console with data saved from a save state
    pub(super) fn load_state(&mut self, slot: u8) {
        if self.config.save_enabled {
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
                    match load() {
                        Ok(()) => self.add_message(&format!("Loaded Slot {}", slot)),
                        Err(e) => self.add_message(&e.to_string()),
                    }
                }
            }
        } else {
            self.add_message("Saved States Disabled");
        }
    }

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    pub(super) fn save_sram(&mut self) -> NesResult<()> {
        let mapper = self.cpu.bus.mapper.borrow();
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
        let mut mapper = self.cpu.bus.mapper.borrow_mut();
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

    pub(super) fn load_replay(&self) -> NesResult<Vec<Vec<PixEvent>>> {
        if let Some(replay) = &self.config.replay {
            let file = std::fs::File::open(&PathBuf::from(replay))
                .map_err(|e| map_nes_err!("failed to open file {:?}: {}", replay, e))?;
            let mut file = BufReader::new(file);
            let mut buffer: Vec<Vec<PixEvent>> = Vec::new();
            buffer.load(&mut file)?;
            Ok(buffer)
        } else {
            Ok(Vec::new())
        }
    }

    /// Searches for valid NES rom files ending in `.nes`
    ///
    /// If rom_path is a `.nes` file, uses that
    /// If no arg[1], searches current directory for `.nes` files
    pub(super) fn find_roms(&self) -> NesResult<Vec<String>> {
        use std::ffi::OsStr;
        let path = PathBuf::from(self.config.path.to_owned());
        let mut roms: Vec<String> = Vec::new();
        if path.is_dir() {
            path.read_dir()
                .map_err(|e| map_nes_err!("unable to read directory {:?}: {}", path, e))?
                .filter_map(|f| f.ok())
                .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
                .for_each(|f| {
                    if let Some(p) = f.path().to_str() {
                        roms.push(p.to_string())
                    }
                });
        } else if path.is_file() {
            if let Some(p) = path.to_str() {
                roms.push(p.to_string());
            } else {
                nes_err!("invalid path: {:?}", path)?;
            }
        } else {
            nes_err!("invalid path: {:?}", path)?;
        }
        if roms.is_empty() {
            nes_err!("no rom files found or specified in {:?}", path)
        } else {
            Ok(roms)
        }
    }
}

impl Powered for Nes {
    /// Soft-resets the console
    fn reset(&mut self) {
        self.cpu.reset();
        self.clock = 0.0;
        self.cycles_remaining = 0.0;
        if self.config.debug {
            self.paused(true);
        }
    }

    /// Hard-resets the console
    fn power_cycle(&mut self) {
        self.cpu.power_cycle();
        self.clock = 0.0;
        self.cycles_remaining = 0.0;
        if self.config.debug {
            self.paused(true);
        }
    }
}

impl Clocked for Nes {
    /// Steps the console a single CPU instruction at a time
    fn clock(&mut self) -> usize {
        if self.config.debug && self.should_break() {
            if self.break_instr == Some(self.cpu.pc) {
                self.break_instr = None;
            } else {
                self.paused(true);
                self.cpu_break = true;
                self.break_instr = Some(self.cpu.pc);
                return 0;
            }
        }
        if self.zapper_decay > 0 {
            self.zapper_decay -= 1;
            // println!(
            //     "decay: {}, sense: {}, sl: {}",
            //     self.zapper_decay, self.cpu.bus.input.zapper.light_sense, self.cpu.bus.ppu.scanline
            // );
        }
        if self.zapper_decay == 0 {
            self.cpu.bus.input.zapper.light_sense = true;
        }
        self.cpu.clock()
    }
}

impl Loggable for Nes {
    fn set_log_level(&mut self, level: LogLevel) {
        self.config.log_level = level;
    }
    fn log_level(&self) -> LogLevel {
        self.config.log_level
    }
}

impl Savable for Nes {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        // Ignore roms/loaded_rom/paused
        self.clock.save(fh)?;
        self.turbo_clock.save(fh)?;
        self.cpu.save(fh)?;
        self.cycles_remaining.save(fh)?;
        // Ignore
        // focused_window
        // lost_focus
        // menu
        // cpu_break
        // break_instr
        // ppu_viewer
        // nt_viewer
        // nt_scanline
        // ppu_viewer_window
        // pat_scanline
        // nt_viewer_window
        // active_debug
        self.width.save(fh)?;
        self.height.save(fh)?;
        self.speed_counter.save(fh)?;
        // Ignore
        // rewind_timer
        // rewind_slot
        // rewind_save
        // rewind_queue
        // record_frame
        // recording
        // playback
        // record_buffer
        // messages
        self.config.save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        // Clone here prevents data corruption if loading fails
        let mut nes = self.clone();
        // Ignore roms/loaded_rom/paused
        nes.clock.load(fh)?;
        nes.turbo_clock.load(fh)?;
        nes.cpu.load(fh)?;
        nes.cycles_remaining.load(fh)?;
        // Ignore
        // focused_window
        // lost_focus
        // menu
        // cpu_break
        // break_instr
        // ppu_viewer
        // nt_viewer
        // nt_scanline
        // ppu_viewer_window
        // pat_scanline
        // nt_viewer_window
        // active_debug
        nes.width.load(fh)?;
        nes.height.load(fh)?;
        nes.speed_counter.load(fh)?;
        // Ignore
        // rewind_timer
        // rewind_slot
        // rewind_save
        // rewind_queue
        // record_frame
        // recording
        // playback
        // record_buffer
        // messages
        nes.config.load(fh)?;
        *self = nes;
        Ok(())
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
    path.set_extension("dat");
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
    let save_name = path.as_ref().file_stem().and_then(|s| s.to_str()).unwrap();
    let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
    path.push(CONFIG_DIR);
    path.push("save");
    path.push(save_name);
    path.push(format!("{}", slot));
    path.set_extension("dat");
    Ok(path)
}
