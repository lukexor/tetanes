//! An NES emulator

use crate::Result;
use cartridge::Cartridge;
use cpu::Cpu;
use input::Input;
use memory::CpuMemMap;
use ppu::{StepResult, RENDER_SIZE};
use sdl2::EventPump;
use std::{fmt, path::Path};

mod cartridge;
mod cpu;
pub mod input;
mod mapper;
mod memory;
mod ppu;

/// The NES Console
///
/// Contains all the components of the console like the CPU, PPU, APU, Cartridge, and Controllers
pub struct Console {
    cpu: Cpu,
}

impl Console {
    /// Creates a new Console instance and maps the appropriate memory address spaces
    pub fn new() -> Self {
        let cpu_memory = CpuMemMap::init();
        Self {
            cpu: Cpu::init(cpu_memory),
        }
    }

    /// Load a cartridge from a ROM file on disk representing an NES cart
    ///
    /// NES ROM files usually end with `.nes`
    pub fn load_cartridge(&mut self, rom: &Path) -> Result<()> {
        let board = Cartridge::new(rom)?.load_board()?;
        self.cpu.set_board(board.clone());
        self.reset();
        Ok(())
    }

    pub fn load_input(&mut self, event_pump: EventPump) {
        let input = Input::init(event_pump);
        self.cpu.mem.set_input(input);
    }

    pub fn poll_events(&mut self) {
        if let Some(input) = &mut self.cpu.mem.input {
            input.poll_events();
        }
    }

    pub fn reset(&mut self) {
        self.cpu.reset();
    }

    pub fn step(&mut self) -> StepResult {
        self.cpu.step()
    }

    pub fn render(&self) -> [u8; RENDER_SIZE] {
        self.cpu.mem.ppu.render()
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
        let mut console = Console::new();
        console
            .load_cartridge(&PathBuf::from(ROMS[index]))
            .expect("cartridge loaded");
        console
    }

    #[test]
    fn test_nestest() {
        let rom = PathBuf::from("tests/cpu/nestest.nes");
        let cpu_log = "logs/cpu.log";
        let nestest_log = "tests/cpu/nestest.txt";

        let mut c = Console::new();
        c.trace_cpu();
        let err = c.load_cartridge(&rom);
        assert!(err.is_ok(), "Load cartridge");

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
