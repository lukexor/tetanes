//! An NES emulator

pub type InputRef = Rc<RefCell<Input>>;
pub use cpu::Cycles;
pub use input::{Input, InputResult};
pub use memory::Memory;
pub use ppu::Image;

use crate::Result;
use cartridge::Cartridge;
use cpu::Cpu;
use memory::CpuMemMap;
use ppu::StepResult;
use std::cell::RefCell;
use std::rc::Rc;
use std::{fmt, path::Path};

mod apu;
mod cartridge;
pub mod cpu;
mod input;
mod mapper;
mod memory;
mod ppu;

const CYCLES_PER_FRAME: Cycles = 29781;

/// The NES Console
///
/// Contains all the components of the console like the CPU, PPU, APU, Cartridge, and Controllers
pub struct Console {
    cpu: Cpu,
    powered_on: bool,
    logging_enabled: bool,
}

impl Console {
    /// Creates a new Console instance and maps the appropriate memory address spaces
    pub fn power_on(rom: &Path, input: InputRef) -> Result<Self> {
        let board = Cartridge::new(rom)?.load_board()?;
        let cpu_memory = CpuMemMap::init(board, input);
        Ok(Self {
            cpu: Cpu::init(cpu_memory),
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
    pub fn step(&mut self) {
        let cpu_cycles = self.cpu.step();
        // Step PPU and Cartridge Board 3x
        for _ in 0..cpu_cycles * 3 {
            let ppu_result = self.cpu.mem.ppu.step();
            {
                let mut board = self.cpu.mem.board.borrow_mut();
                board.step();
            }
            if ppu_result.trigger_nmi {
                self.cpu.nmi();
            } else if ppu_result.trigger_irq {
                self.cpu.irq();
            }
        }
        // Step APU
        for _ in 0..cpu_cycles {
            self.cpu.mem.apu.step();
        }
    }
    pub fn step_frame(&mut self) {
        for _ in 0..CYCLES_PER_FRAME {
            self.step();
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
        &mut self.cpu.mem.apu.samples
    }
}

impl fmt::Debug for Console {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Console {{\n  cpu: {:?}\n}} ", self.cpu)
    }
}

#[cfg(test)]
use crate::console::memory::Addr;

#[cfg(test)]
impl Console {
    pub fn set_pc(&mut self, addr: Addr) {
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

    const NESTEST_ADDR: Addr = 0xC000;
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
