//! Handles NES Control Deck operations

pub use apu::{SAMPLE_BUFFER_SIZE, SAMPLE_RATE};
pub use cpu::CPU_CLOCK_RATE;
pub use ppu::{Image, Rgb, RENDER_HEIGHT, RENDER_WIDTH};

use crate::input::InputRef;
use crate::mapper::{self, MapperRef};
use crate::memory::{self, CpuMemMap};
use crate::serialization::Savable;
use crate::util::{self, Result};
use cpu::Cpu;
use failure::format_err;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

pub mod apu;
pub mod cpu;
pub mod debugger;
pub mod ppu;

/// Represents the NES Control Deck
///
/// Manages all the components of the console like the CPU, PPU, APU, Cartridge, and Controllers
pub struct Console {
    no_save: bool,
    loaded_rom: PathBuf,
    pub cpu: Box<Cpu>,
    mapper: MapperRef,
}

impl Console {
    /// Creates a new Console instance and maps the appropriate memory address spaces
    pub fn init(input: InputRef) -> Self {
        let cpu_memory = CpuMemMap::init(input);
        let mut cpu = Box::new(Cpu::init(cpu_memory));
        cpu.mem.apu.dmc.cpu = (&mut *cpu) as *mut Cpu; // TODO ugly work-around for DMC memory
        Self {
            no_save: false,
            loaded_rom: PathBuf::new(),
            cpu,
            mapper: mapper::null(),
        }
    }

    /// Loads a ROM cartridge into memory
    pub fn load_rom<P: AsRef<Path>>(&mut self, rom: P) -> Result<()> {
        self.loaded_rom = rom.as_ref().to_path_buf();
        let mapper = mapper::load_rom(rom)?;
        self.mapper = mapper.clone();
        self.cpu.mem.load_mapper(mapper);
        Ok(())
    }

    /// Powers on the console
    pub fn power_on(&mut self) -> Result<()> {
        self.cpu.power_on();
        self.load_sram()
    }

    /// Powers off the console
    pub fn power_off(&mut self) -> Result<()> {
        self.save_sram()
    }

    /// Steps the console the number of instructions required to generate an entire frame
    pub fn clock_frame(&mut self) {
        let mut cycles_remaining = (CPU_CLOCK_RATE / 60.0) as i64;
        while cycles_remaining > 0 {
            cycles_remaining -= self.clock() as i64;
        }
    }

    /// Soft-resets the console
    pub fn reset(&mut self) {
        self.cpu.reset();
        self.mapper.borrow_mut().reset();
    }

    /// Hard-resets the console
    pub fn power_cycle(&mut self) {
        self.cpu.power_cycle();
        self.mapper.borrow_mut().power_cycle();
    }

    /// Enable/Disable RAM randomization
    pub fn randomize_ram(&mut self, val: bool) {
        unsafe { memory::RANDOMIZE_RAM = val }
    }

    /// Enable/Disable the debugger
    pub fn debug(&mut self, val: bool) {
        self.cpu.debug(val);
    }

    /// Enable/Disable CPU logging
    pub fn log_cpu(&mut self, val: bool) {
        self.cpu.log(val);
    }

    /// Enable/Disable Save states
    pub fn no_save(&mut self, val: bool) {
        self.no_save = val;
    }

    /// Returns a rendered frame worth of data from the PPU
    pub fn render(&self) -> Image {
        self.cpu.mem.ppu.render()
    }

    pub fn default_bg_color(&mut self) -> Rgb {
        self.cpu.mem.ppu.default_bg_color()
    }

    /// Returns a frame worth of audio samples from the APU
    pub fn audio_samples(&mut self) -> &mut Vec<f32> {
        self.cpu.mem.apu.samples()
    }

    /// Changes the running speed of the console
    pub fn set_speed(&mut self, speed: f64) {
        self.cpu.mem.apu.set_speed(speed);
    }

    /// Save the current state of the console into a save file
    pub fn save_state(&mut self, slot: u8) -> Result<()> {
        if self.no_save {
            return Ok(());
        }
        let save_path = util::save_path(&self.loaded_rom, slot)?;
        let save_dir = save_path.parent().unwrap(); // Safe to do because save_path is never root
        if !save_dir.exists() {
            fs::create_dir_all(save_dir).map_err(|e| {
                format_err!("failed to create directory {:?}: {}", save_dir.display(), e)
            })?;
        }
        let save_file = fs::File::create(&save_path)
            .map_err(|e| format_err!("failed to create file {:?}: {}", save_path.display(), e))?;
        let mut writer = BufWriter::new(save_file);
        util::write_save_header(&mut writer)
            .map_err(|e| format_err!("failed to write header {:?}: {}", save_path.display(), e))?;
        self.save(&mut writer)?;
        Ok(())
    }

    /// Load the console with data saved from a save state
    pub fn load_state(&mut self, slot: u8) -> Result<()> {
        if self.no_save {
            return Ok(());
        }
        let save_path = util::save_path(&self.loaded_rom, slot)?;
        if save_path.exists() {
            let save_file = fs::File::open(&save_path)
                .map_err(|e| format_err!("failed to open file {:?}: {}", save_path.display(), e))?;
            let mut reader = BufReader::new(save_file);
            match util::validate_save_header(&mut reader) {
                Ok(_) => self.load(&mut reader)?,
                Err(e) => eprintln!("failed to load save slot #{}: {}", slot, e),
            }
        }
        Ok(())
    }

    /// Steps the console a single CPU instruction at a time
    fn clock(&mut self) -> u64 {
        let cpu_cycles = self.cpu.clock();
        let ppu_cycles = cpu_cycles * 3;
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
            if irq_pending {
                self.cpu.trigger_irq();
            }
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
    fn save_sram(&mut self) -> Result<()> {
        if self.no_save {
            return Ok(());
        }
        let mapper = self.cpu.mem.mapper.borrow();
        if mapper.battery_backed() {
            let sram_path = util::sram_path(&self.loaded_rom)?;
            let sram_dir = sram_path.parent().unwrap(); // Safe to do because sram_path is never root
            if !sram_dir.exists() {
                fs::create_dir_all(sram_dir).map_err(|e| {
                    format_err!("failed to create directory {:?}: {}", sram_dir.display(), e)
                })?;
            }

            let mut sram_file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&sram_path)
                .map_err(|e| format_err!("failed to open file {:?}: {}", sram_path.display(), e))?;

            // Empty file means we just created it
            if sram_file.metadata()?.len() == 0 {
                util::write_save_header(&mut sram_file).map_err(|e| {
                    format_err!("failed to write header {:?}: {}", sram_path.display(), e)
                })?;
                mapper.save_sram(&mut sram_file)?;
            } else {
                // Check if exists and header is different, so we avoid overwriting
                match util::validate_save_header(&mut sram_file) {
                    Ok(_) => {
                        mapper.save_sram(&mut sram_file)?;
                    }
                    Err(e) => eprintln!("failed to write sram due to invalid header. error: {}", e),
                }
            }
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    fn load_sram(&mut self) -> Result<()> {
        if self.no_save {
            return Ok(());
        }
        let mut mapper = self.mapper.borrow_mut();
        if mapper.battery_backed() {
            let sram_path = util::sram_path(&self.loaded_rom)?;
            if sram_path.exists() {
                let mut sram_file = fs::File::open(&sram_path).map_err(|e| {
                    format_err!("failed to open file {:?}: {}", sram_path.display(), e)
                })?;
                match util::validate_save_header(&mut sram_file) {
                    Ok(_) => {
                        mapper.load_sram(&mut sram_file)?;
                    }
                    Err(e) => eprintln!(
                        "failed to load sram: {}.\n  move or delete `{}` before exiting, otherwise sram data will be lost.",
                        e,
                        sram_path.display()
                    ),
                }
            }
        }
        Ok(())
    }
}

impl Savable for Console {
    fn save(&self, fh: &mut Write) -> Result<()> {
        self.cpu.save(fh)
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
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
    use super::*;
    use crate::input::Input;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::{fs, path::PathBuf};

    const NESTEST_ADDR: u16 = 0xC000;
    const NESTEST_LEN: usize = 8980;

    #[test]
    fn test_nestest() {
        let rom = PathBuf::from("tests/cpu/nestest.nes");
        let cpu_log = "logs/nestest.log";
        let nestest_log = "tests/cpu/nestest.txt";

        let input = Rc::new(RefCell::new(Input::new()));
        let mut c = Console::init(input);
        c.load_rom(rom).expect("loaded rom");
        c.power_on().expect("powered on");
        c.cpu.nestest = true;

        c.cpu.pc = NESTEST_ADDR;
        for _ in 0..NESTEST_LEN {
            c.clock();
        }
        let log = c.cpu.nestestlog.join("");
        fs::write(cpu_log, &log).expect("Failed to write nestest.log");

        let nestest = fs::read_to_string(nestest_log);
        assert!(nestest.is_ok(), "Read nestest");
        let equal = if log == nestest.unwrap() { true } else { false };
        assert!(equal, "CPU log matches nestest");
    }
}
