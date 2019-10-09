//! Handles NES Control Deck operations

pub use apu::{SAMPLE_BUFFER_SIZE, SAMPLE_RATE};
pub use cpu::CPU_CLOCK_RATE;
pub use ppu::{RENDER_HEIGHT, RENDER_WIDTH};

use crate::input::InputRef;
use crate::mapper::{self, MapperRef};
use crate::memory::{self, Memory, MemoryMap};
use crate::serialization::Savable;
use crate::util;
use crate::{map_nes_err, NesResult};
use cpu::Cpu;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

pub mod apu;
pub mod cpu;
pub mod ppu;

/// Represents the NES Control Deck
///
/// Manages all the components of the console like the CPU, PPU, APU, Cartridge, and Controllers
pub struct Console {
    running: bool,
    loaded_rom: PathBuf,
    pub cpu: Cpu<MemoryMap>,
    cycles_remaining: f64,
    mapper: MapperRef,
}

impl Console {
    /// Creates a new Console instance and maps the appropriate memory address spaces
    pub fn init(input: InputRef, randomize_ram: bool) -> Self {
        unsafe { memory::RANDOMIZE_RAM = randomize_ram }
        let memory_map = MemoryMap::init(input);
        let cpu = Cpu::init(memory_map);
        Self {
            running: false,
            loaded_rom: PathBuf::new(),
            cpu,
            cycles_remaining: 0.0,
            mapper: mapper::null(),
        }
    }

    /// Loads a ROM cartridge into memory
    pub fn load_rom<P: AsRef<Path>>(&mut self, rom: P) -> NesResult<()> {
        self.loaded_rom = rom.as_ref().to_path_buf();
        let mapper = mapper::load_rom(rom)?;
        self.mapper = mapper.clone();
        self.cpu.mem.load_mapper(mapper);
        Ok(())
    }

    /// Powers on the console
    pub fn power_on(&mut self) -> NesResult<()> {
        self.cpu.power_on();
        self.load_sram()?;
        self.running = true;
        self.cycles_remaining = 0.0;
        Ok(())
    }

    /// Powers off the console
    pub fn power_off(&mut self) -> NesResult<()> {
        self.save_sram()?;
        self.power_cycle();
        self.running = false;
        Ok(())
    }

    /// Steps the console the number of instructions required to generate an entire frame
    pub fn clock_frame(&mut self) {
        if self.running {
            while !self.cpu.mem.ppu.frame_complete {
                let _ = self.clock();
            }
            self.cpu.mem.ppu.frame_complete = false;
        }
    }

    pub fn clock_seconds(&mut self, seconds: f64) {
        if self.running {
            self.cycles_remaining += CPU_CLOCK_RATE * seconds;
            while self.cycles_remaining > 0.0 {
                self.cycles_remaining -= self.clock() as f64;
            }
        }
    }

    /// Soft-resets the console
    pub fn reset(&mut self) {
        self.logging(false);
        self.cpu.reset();
    }

    /// Hard-resets the console
    pub fn power_cycle(&mut self) {
        self.logging(false);
        self.cpu.power_cycle();
    }

    /// Enable/Disable CPU logging
    pub fn logging(&mut self, val: bool) {
        self.cpu.logging(val);
        self.cpu.mem.ppu.logging(val);
        self.mapper.borrow_mut().logging(val);
    }

    pub fn ppu_debug(&mut self, val: bool) {
        self.cpu.mem.ppu.debug(val);
    }

    /// Returns a rendered frame worth of data from the PPU
    pub fn frame(&self) -> Vec<u8> {
        self.cpu.mem.ppu.frame()
    }

    /// Returns nametable graphics
    pub fn nametables(&self) -> &Vec<Vec<u8>> {
        self.cpu.mem.ppu.nametables()
    }

    /// Returns pattern table graphics
    pub fn pattern_tables(&self) -> &Vec<Vec<u8>> {
        self.cpu.mem.ppu.pattern_tables()
    }

    /// Returns palette graphics
    pub fn palettes(&self) -> &Vec<Vec<u8>> {
        self.cpu.mem.ppu.palettes()
    }

    /// Returns a frame worth of audio samples from the APU
    pub fn audio_samples(&mut self) -> &[f32] {
        self.cpu.mem.apu.samples()
    }

    pub fn clear_audio(&mut self) {
        self.cpu.mem.apu.clear_samples()
    }

    /// Changes the running speed of the console
    pub fn set_speed(&mut self, speed: f64) {
        self.cpu.mem.apu.set_speed(speed);
    }

    /// Save the current state of the console into a save file
    pub fn save_state(&mut self, slot: u8) -> NesResult<()> {
        let save_path = util::save_path(&self.loaded_rom, slot)?;
        let save_dir = save_path.parent().unwrap(); // Safe to do because save_path is never root
        if !save_dir.exists() {
            fs::create_dir_all(save_dir).map_err(|e| {
                map_nes_err!("failed to create directory {:?}: {}", save_dir.display(), e)
            })?;
        }
        let save_file = fs::File::create(&save_path)
            .map_err(|e| map_nes_err!("failed to create file {:?}: {}", save_path.display(), e))?;
        let mut writer = BufWriter::new(save_file);
        util::write_save_header(&mut writer)
            .map_err(|e| map_nes_err!("failed to write header {:?}: {}", save_path.display(), e))?;
        self.save(&mut writer)?;
        Ok(())
    }

    /// Load the console with data saved from a save state
    pub fn load_state(&mut self, slot: u8) -> NesResult<()> {
        let save_path = util::save_path(&self.loaded_rom, slot)?;
        if save_path.exists() {
            let save_file = fs::File::open(&save_path).map_err(|e| {
                map_nes_err!("failed to open file {:?}: {}", save_path.display(), e)
            })?;
            let mut reader = BufReader::new(save_file);
            match util::validate_save_header(&mut reader) {
                Ok(_) => {
                    if let Err(e) = self.load(&mut reader) {
                        eprintln!("failed to load save slot #{}: {}", slot, e);
                        self.reset();
                    }
                }
                Err(e) => eprintln!("failed to load save slot #{}: {}", slot, e),
            }
        }
        Ok(())
    }

    /// Steps the console a single CPU instruction at a time
    pub fn clock(&mut self) -> u64 {
        let cpu_cycles = self.cpu.clock();
        let ppu_cycles = 3 * cpu_cycles;

        for _ in 0..ppu_cycles {
            self.cpu.mem.ppu.clock();
            if self.cpu.mem.ppu.nmi_pending {
                self.cpu.trigger_nmi();
                self.cpu.mem.ppu.nmi_pending = false;
            }

            let irq_pending = {
                let mut mapper = self.cpu.mem.mapper.borrow_mut();
                mapper.clock(&self.cpu.mem.ppu);
                mapper.irq_pending()
            };
            self.cpu.trigger_irq2(irq_pending);
        }

        for _ in 0..cpu_cycles {
            self.cpu.mem.apu.clock();
            if self.cpu.mem.apu.irq_pending {
                self.cpu.trigger_irq();
                self.cpu.mem.apu.irq_pending = false;
            }
        }

        cpu_cycles
    }

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    fn save_sram(&mut self) -> NesResult<()> {
        let mapper = self.cpu.mem.mapper.borrow();
        if mapper.battery_backed() {
            let sram_path = util::sram_path(&self.loaded_rom)?;
            let sram_dir = sram_path.parent().unwrap(); // Safe to do because sram_path is never root
            if !sram_dir.exists() {
                fs::create_dir_all(sram_dir).map_err(|e| {
                    map_nes_err!("failed to create directory {:?}: {}", sram_dir.display(), e)
                })?;
            }

            let mut sram_opts = fs::OpenOptions::new()
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
                util::write_save_header(&mut sram_file).map_err(|e| {
                    map_nes_err!("failed to write header {:?}: {}", sram_path.display(), e)
                })?;
                mapper.save_sram(&mut sram_file)?;
            } else {
                // Check if exists and header is different, so we avoid overwriting
                match util::validate_save_header(&mut sram_opts) {
                    Ok(_) => {
                        let mut sram_file = BufWriter::new(sram_opts);
                        mapper.save_sram(&mut sram_file)?;
                    }
                    Err(e) => eprintln!("failed to write sram due to invalid header. error: {}", e),
                }
            }
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    fn load_sram(&mut self) -> NesResult<()> {
        let mut load_failure = false;
        {
            let mut mapper = self.mapper.borrow_mut();
            if mapper.battery_backed() {
                let sram_path = util::sram_path(&self.loaded_rom)?;
                if sram_path.exists() {
                    let sram_file = fs::File::open(&sram_path).map_err(|e| {
                        map_nes_err!("failed to open file {:?}: {}", sram_path.display(), e)
                    })?;
                    let mut sram_file = BufReader::new(sram_file);
                    match util::validate_save_header(&mut sram_file) {
                        Ok(_) => {
                            if let Err(e) = mapper.load_sram(&mut sram_file) {
                                eprintln!("failed to load save sram: {}", e);
                                load_failure = true;
                            }
                        }
                        Err(e) => eprintln!(
                            "failed to load sram: {}.\n  move or delete `{}` before exiting, otherwise sram data will be lost.",
                            e,
                            sram_path.display()
                        ),
                    }
                }
            }
        }
        if load_failure {
            self.reset();
        }
        Ok(())
    }
}

impl Savable for Console {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.running.save(fh)?;
        self.cpu.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.running.load(fh)?;
        self.cpu.load(fh)
    }
}

impl fmt::Debug for Console {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Console {{\n  cpu: {:?}\n}} ", self.cpu)
    }
}

#[cfg(test)]
mod tests {
    // TODO
}
