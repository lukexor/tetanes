//! Handles NES Control Deck operations

pub use apu::{SAMPLE_BUFFER_SIZE, SAMPLE_RATE};
pub use cpu::CPU_CLOCK_RATE;
pub use ppu::{Image, SCREEN_HEIGHT, SCREEN_WIDTH};

use crate::cartridge::RAM_SIZE;
use crate::input::{InputRef, InputResult};
use crate::mapper;
use crate::memory::CpuMemMap;
use crate::serialization::Savable;
use crate::util::{self, Result};
use cpu::Cpu;
use failure::format_err;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::{fmt, fs};

pub mod apu;
pub mod cpu;
pub mod debugger;
pub mod ppu;

/// Represents the NES Control Deck
///
/// Manages all the components of the console like the CPU, PPU, APU, Cartridge, and Controllers
pub struct Console {
    cpu: Cpu,
    cycles_remaining: i64,
}

impl Console {
    /// Creates a new Console instance and maps the appropriate memory address spaces
    pub fn power_on(rom: PathBuf, input: InputRef) -> Result<Self> {
        let mapper = mapper::load_rom(rom)?;
        let cpu_memory = CpuMemMap::init(mapper, input);
        let mut console = Self {
            cpu: Cpu::init(cpu_memory),
            cycles_remaining: 0,
        };
        console.load_sram()?;
        Ok(console)
    }

    /// Steps the console the number of instructions required to generate an entire frame
    pub fn step_frame(&mut self) {
        self.cycles_remaining += (CPU_CLOCK_RATE / 60.0) as i64;
        while self.cycles_remaining > 0 {
            self.cycles_remaining -= self.step() as i64;
        }
    }

    /// Powers off the console
    pub fn power_off(&mut self) -> Result<()> {
        self.save_sram()?;
        Ok(())
    }

    /// Soft-resets the console
    pub fn reset(&mut self) {
        self.cpu.reset();
    }

    /// Hard-resets the console
    pub fn power_cycle(&mut self) {
        self.cpu.power_cycle();
    }

    /// Enable/Disable the debugger
    pub fn debug(&mut self, val: bool) {
        self.cpu.debug(val);
    }

    /// Returns a rendered frame worth of data from the PPU
    pub fn render(&self) -> Image {
        self.cpu.mem.ppu.render()
    }

    /// Returns a frame worth of audio samples from the APU
    pub fn audio_samples(&mut self) -> &mut Vec<f32> {
        self.cpu.mem.apu.samples()
    }

    /// Process input events and return a result
    pub fn poll_events(&mut self) -> InputResult {
        let mut input = self.cpu.mem.input.borrow_mut();
        let turbo = self.cpu.mem.ppu.frame() % 6 < 3;
        input.poll_events(turbo)
    }

    /// Changes the running speed of the console
    pub fn set_speed(&mut self, speed: f64) {
        self.cpu.mem.apu.set_speed(speed);
    }

    /// Steps the console a single CPU instruction at a time
    fn step(&mut self) -> u64 {
        let cpu_cycles = self.cpu.step();
        for _ in 0..cpu_cycles * 3 {
            self.cpu.mem.ppu.clock();
            if self.cpu.mem.ppu.nmi_pending {
                self.cpu.trigger_nmi();
                self.cpu.mem.ppu.nmi_pending = false;
            }
            let irq_pending = {
                let mut mapper = self.cpu.mem.mapper.borrow_mut();
                mapper.step(&self.cpu.mem.ppu);
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

    /// Load the console with data saved from a save state
    pub fn load_state(&mut self, slot: u8) -> Result<()> {
        let rom_file = {
            let mapper = self.cpu.mem.mapper.borrow();
            mapper.cart().rom_file.clone()
        };
        let save_path = util::save_path(&rom_file, slot)?;
        if save_path.exists() {
            let save_file = fs::File::open(&save_path).map_err(|e| {
                format_err!("failed to open save file {:?}: {}", save_path.display(), e)
            })?;
            let mut reader = BufReader::new(save_file);
            self.load(&mut reader)?;
            eprintln!("Loaded #{}", slot);
        }
        Ok(())
    }

    /// Save the current state of the console into a save file
    pub fn save_state(&mut self, slot: u8) -> Result<()> {
        let mapper = self.cpu.mem.mapper.borrow();
        let rom_file = &mapper.cart().rom_file;
        let save_path = util::save_path(rom_file, slot)?;
        let save_dir = save_path.parent().unwrap(); // Safe to do
        if !save_dir.exists() {
            fs::create_dir_all(save_dir).map_err(|e| {
                format_err!(
                    "failed to create save directory {:?}: {}",
                    save_dir.display(),
                    e
                )
            })?;
        }
        let save_file = fs::File::create(&save_path).map_err(|e| {
            format_err!("failed to open save file {:?}: {}", save_path.display(), e)
        })?;
        let mut writer = BufWriter::new(save_file);
        self.save(&mut writer)?;
        eprintln!("Saved #{}", slot);
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    fn load_sram(&mut self) -> Result<()> {
        let mut mapper = self.cpu.mem.mapper.borrow_mut();
        if mapper.cart().has_battery() {
            let rom_file = &mapper.cart().rom_file;
            let sram_path = util::sram_path(rom_file)?;
            if sram_path.exists() {
                let mut sram_file = fs::File::open(&sram_path).map_err(|e| {
                    format_err!("failed to open sram file {:?}: {}", sram_path.display(), e)
                })?;
                let mut sram = Vec::with_capacity(RAM_SIZE);
                sram_file.read_to_end(&mut sram).map_err(|e| {
                    format_err!("failed to read sram file {:?}: {}", sram_path.display(), e)
                })?;
                mapper.cart_mut().sram = sram;
            }
        }
        Ok(())
    }

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    fn save_sram(&mut self) -> Result<()> {
        let mapper = self.cpu.mem.mapper.borrow();
        if mapper.cart().has_battery() {
            let rom_file = &mapper.cart().rom_file;
            let sram_path = util::sram_path(rom_file)?;
            let sram_dir = sram_path.parent().unwrap(); // Safe to do
            if !sram_dir.exists() {
                fs::create_dir_all(sram_dir).map_err(|e| {
                    format_err!(
                        "failed to create sram directory {:?}: {}",
                        sram_dir.display(),
                        e
                    )
                })?;
            }
            fs::write(&sram_path, &mapper.cart().sram).map_err(|e| {
                format_err!("failed to write sram file {:?}: {}", sram_path.display(), e)
            })?;
        }
        Ok(())
    }
}

impl Savable for Console {
    fn save(&self, fh: &mut Write) -> Result<()> {
        self.cpu.save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        self.cpu.load(fh)?;
        Ok(())
    }
}

impl fmt::Debug for Console {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Console {{\n  cpu: {:?}\n}} ", self.cpu)
    }
}

#[cfg(test)]
impl Console {
    pub fn set_pc(&mut self, addr: u16) {
        self.cpu.set_pc(addr);
    }

    fn set_nestest(&mut self) {
        self.cpu.set_nestest(true);
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
        let mut c = Console::power_on(rom, input).expect("powered on");
        c.set_nestest();

        c.set_pc(NESTEST_ADDR);
        for _ in 0..NESTEST_LEN {
            c.step();
        }
        let log = c.cpu.nestestlog.join("");
        fs::write(cpu_log, &log).expect("Failed to write nestest.log");

        let nestest = fs::read_to_string(nestest_log);
        assert!(nestest.is_ok(), "Read nestest");
        let equal = log == nestest.unwrap();
        assert!(equal, "CPU log matches nestest");
    }
}
