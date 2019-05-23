//! An NES emulator

pub use apu::{SAMPLES_SIZE, SAMPLE_RATE};
pub use ppu::{Image, SCREEN_HEIGHT, SCREEN_WIDTH};

use crate::input::{InputRef, InputResult};
use crate::mapper;
use crate::memory::CpuMemMap;
use crate::Result;
use cpu::Cpu;
use std::{fmt, path::PathBuf};

pub mod apu;
pub mod cpu;
pub mod debugger;
pub mod ppu;

pub const MASTER_CLOCK_RATE: f64 = 21_477_270.0; // 21.47727 MHz
const CPU_FREQUENCY: f64 = MASTER_CLOCK_RATE / 12.0; // 1.7897725 MHz

/// The NES Console
///
/// Contains all the components of the console like the CPU, PPU, APU, Cartridge, and Controllers
pub struct Console {
    cpu: Cpu,
    cycles_remaining: i64,
}

impl Console {
    /// Creates a new Console instance and maps the appropriate memory address spaces
    pub fn power_on(rom: PathBuf, input: InputRef) -> Result<Self> {
        let mapper = mapper::load_rom(rom)?;
        let cpu_memory = CpuMemMap::init(mapper, input);
        Ok(Self {
            cpu: Cpu::init(cpu_memory),
            cycles_remaining: 0,
        })
    }
    pub fn power_cycle(&mut self) {
        self.cpu.power_cycle();
    }
    pub fn reset(&mut self) {
        self.cpu.reset();
    }
    pub fn debug(&mut self, val: bool) {
        self.cpu.debug(val);
    }
    pub fn step(&mut self) -> u64 {
        let cpu_cycles = self.cpu.step();
        for _ in 0..cpu_cycles * 3 {
            self.cpu.mem.ppu.clock();
            if self.cpu.mem.ppu.nmi_pending {
                self.cpu.trigger_nmi();
                self.cpu.mem.ppu.nmi_pending = false;
            }
            let mut mapper = self.cpu.mem.mapper.borrow_mut();
            mapper.step();
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
    pub fn step_frame(&mut self, speed: u16) {
        self.cycles_remaining += (CPU_FREQUENCY / speed as f64) as i64;
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
