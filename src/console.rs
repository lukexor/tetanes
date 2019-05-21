//! An NES emulator

pub use ppu::Image;

use crate::cartridge::Cartridge;
use crate::input::{InputRef, InputResult};
use crate::mapper;
use crate::memory::CpuMemMap;
use crate::Result;
use cpu::Cpu;
use ppu::StepResult;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use std::{fmt, path::PathBuf};

pub mod apu;
pub mod cpu;
pub mod ppu;

pub const MASTER_CLOCK_RATE: f64 = 21_477_270.0; // 21.47727 MHz
const CPU_FREQUENCY: f64 = MASTER_CLOCK_RATE / 12.0; // 1.7897725 MHz
const REFRESH_RATE: f64 = 60.0; // 60 Hz

/// The NES Console
///
/// Contains all the components of the console like the CPU, PPU, APU, Cartridge, and Controllers
pub struct Console {
    cpu: Cpu,
    cycles_remaining: i64,
    powered_on: bool,
    logging_enabled: bool,
}

impl Console {
    /// Creates a new Console instance and maps the appropriate memory address spaces
    pub fn power_on(rom: PathBuf, input: InputRef) -> Result<Self> {
        let mapper = mapper::load_rom(rom)?;
        let cpu_memory = CpuMemMap::init(mapper, input);
        Ok(Self {
            cpu: Cpu::init(cpu_memory),
            cycles_remaining: 0,
            powered_on: true,
            logging_enabled: false,
        })
    }
    pub fn power_cycle(&mut self) {
        self.cpu.power_cycle();
    }
    pub fn reset(&mut self) {
        self.cpu.reset();
    }
    pub fn step(&mut self) -> u64 {
        let cpu_cycles = self.cpu.step();
        // Step PPU and mapper 3x
        let mut ppu_result = StepResult::new();
        for _ in 0..cpu_cycles * 3 {
            ppu_result = self.cpu.mem.ppu.step();
            {
                let mut mapper = self.cpu.mem.mapper.borrow_mut();
                mapper.step();
            }
            if ppu_result.trigger_nmi {
                self.cpu.trigger_nmi();
            } else if ppu_result.trigger_irq {
                self.cpu.trigger_irq();
            }
        }
        // Step APU
        for _ in 0..cpu_cycles {
            self.cpu.mem.apu.clock();
        }
        cpu_cycles
    }
    pub fn step_frame(&mut self) {
        self.cycles_remaining += (CPU_FREQUENCY / REFRESH_RATE) as i64;
        while self.cycles_remaining > 0 {
            self.cycles_remaining -= self.step() as i64;
        }
    }
    pub fn poll_events(&mut self) -> InputResult {
        let mut input = self.cpu.mem.input.borrow_mut();
        input.poll_events()
    }
    pub fn render(&self) -> Image {
        self.cpu.mem.ppu.render()
    }
    pub fn audio_samples(&mut self) -> &mut Vec<f32> {
        self.cpu.mem.apu.samples()
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

    fn trace_cpu(&mut self) {
        self.cpu.set_trace(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    const NESTEST_ADDR: u16 = 0xC000;
    const NESTEST_LEN: usize = 8991;
    const ROMS: &[&str] = &[
        "roms/Zelda II - The Adventure of Link (USA).nes",
        "roms/Super Mario Bros. (World).nes",
        "roms/Metroid (USA).nes",
        "roms/Gauntlet (USA).nes",
    ];

    fn new_game_console(index: usize) -> Console {
        let rom = &PathBuf::from(ROMS[index]);
        let input = Rc::new(RefCell::new(Input::new()));
        let mut console = Console::power_on(rom, input).expect("powered on");
        console
    }

    #[test]
    fn test_nestest() {
        let rom = PathBuf::from("tests/cpu/nestest.nes");
        let cpu_log = "logs/cpu.log";
        let nestest_log = "tests/cpu/nestest.txt";

        let input = Rc::new(RefCell::new(Input::new()));
        let mut c = Console::power_on(&rom, input).expect("powered on");
        c.trace_cpu();

        c.set_pc(NESTEST_ADDR);
        for _ in 0..NESTEST_LEN {
            c.step();
        }
        fs::write(cpu_log, &c.cpu.oplog).expect("Failed to write op.log");

        let nestest = fs::read_to_string(nestest_log);
        assert!(nestest.is_ok(), "Read nestest");
        assert!(c.cpu.oplog == nestest.unwrap(), "CPU log matches nestest");
    }
}
