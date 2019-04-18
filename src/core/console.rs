use super::apu::APU;
use super::cartridge::Cartridge;
use super::controller::Controller;
use super::cpu::{Interrupt, CPU};
use super::cpu_instructions::{execute, php, print_instruction};
use super::mapper::Mapper1;
use super::memory::{push16, read16, read_byte};
use super::ppu::PPU;
use std::error::Error;

const CPU_FREQUENCY: f64 = 1_789_773.0;
const RAM_SIZE: usize = 2048;

/// The NES Console
pub struct Console {
    pub cpu: CPU,
    pub apu: APU,
    pub ppu: PPU,
    pub cartridge: Cartridge,
    pub controller1: Controller,
    pub controller2: Controller,
    // pub mapper: Box<Mapper>,
    pub mapper: Mapper1,
    pub ram: Vec<u8>,
}

impl Console {
    pub fn new(cartridge: Cartridge) -> Result<Self, Box<Error>> {
        let mapper = cartridge.get_mapper()?;
        let mut console = Console {
            cpu: CPU::new(),
            apu: APU::new(),
            ppu: PPU::new(),
            cartridge,
            mapper,
            controller1: Controller::new(),
            controller2: Controller::new(),
            ram: vec![0; RAM_SIZE],
        };
        console.reset();
        Ok(console)
    }

    pub fn reset(&mut self) {
        self.cpu.pc = read_byte(self, 0xFFFC).into();
        self.cpu.sp = 0xFD;
        self.cpu.set_flags(0x24);
    }

    pub fn step_seconds(&mut self, seconds: f64) {
        let mut cycles = (CPU_FREQUENCY * seconds) as u64;
        while cycles > 0 {
            cycles -= self.step();
        }
    }

    pub fn step(&mut self) -> u64 {
        print_instruction(self);
        let cpu_cycles = if self.cpu.stall > 0 {
            self.cpu.stall -= 1;
            1
        } else {
            let start_cycles = self.cpu.cycles;
            match &self.cpu.interrupt {
                Interrupt::NMI => {
                    push16(self, self.cpu.pc);
                    php(self);
                    self.cpu.pc = read16(self, 0xFFFA);
                    self.cpu.i = 1;
                    self.cpu.cycles += 7;
                }
                Interrupt::IRQ => {
                    push16(self, self.cpu.pc);
                    php(self);
                    self.cpu.pc = read16(self, 0xFFFE);
                    self.cpu.i = 1;
                    self.cpu.cycles += 7;
                }
                _ => (),
            }
            self.cpu.interrupt = Interrupt::None;
            let opcode = read_byte(self, self.cpu.pc);
            execute(self, opcode);
            (self.cpu.cycles - start_cycles)
        };
        for _ in 0..cpu_cycles * 3 {
            // TODO self.ppu.step();
            // TODO self.mapper.step();
        }
        for _ in 0..cpu_cycles {
            // TODO self.apu.step();
        }
        cpu_cycles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cpu_instructions::*;
    use crate::core::memory::{pull, pull16, read_byte, write};

    const ROM1: &str = "roms/Zelda II - The Adventure of Link (USA).nes";

    fn new_console() -> Console {
        let cartridge = Cartridge::new(ROM1).expect("valid cartridge");
        Console::new(cartridge).expect("valid console")
    }

    #[test]
    fn test_new_console() {
        // TODO test startup state
    }

    #[test]
    fn test_opcodes() {
        for i in 0u8..=255 {
            let mut c = new_console();
            test_opstate(&mut c, i);
        }
    }

    fn test_opstate(c: &mut Console, opcode: u8) {
        let addr = 0x0100;
        match opcode {
            // BRK - Force Interrupt
            0 => {
                let flags = c.cpu.flags();
                let pc = c.cpu.pc;
                brk(c);
                // Interrupt disable bit set
                assert_eq!(c.cpu.i, 1);
                // Startup processor status is on the stack
                assert_eq!(pull(c), flags | 0x10);
                // pc stored on stack
                assert_eq!(pull16(c), pc);
            }
            // ORA - "OR" M with A
            1 | 5 | 9 | 13 | 17 | 21 | 25 | 29 => {
                // Test cases
                // M | A | M OR A | z | n
                // 0 | 0 | 0      | 1 | 0
                // 1 | 0 | 1      | 0 | 0
                // 0 | 1 | 1      | 0 | 0
                // 1 | 1 | 1      | 0 | 0

                write(c, addr, 0);
                c.cpu.a = 0;
                ora(c, addr);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 1);
                c.cpu.pc = u16::from(opcode);
                c.cpu.a = 0;
                ora(c, addr);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 0);
                c.cpu.pc = u16::from(opcode);
                c.cpu.a = 1;
                ora(c, addr);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 1);
                c.cpu.pc = u16::from(opcode);
                c.cpu.a = 1;
                ora(c, addr);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                c.reset();
            }
            // ASL Shift Left M
            6 | 14 | 22 | 30 => {
                // Test cases
                //            | C | M   | z | n
                // val == 0   | 0 | 0   | 1 | 0
                // val <= 127 | 0 | 2*M | 0 | 0
                // val > 127  | 1 | 2*M | 0 | 0
                write(c, addr, 0);
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 0);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(read_byte(c, addr), 0);
                c.reset();

                write(c, addr, 50);
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 0);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(read_byte(c, addr), 100);
                c.reset();

                write(c, addr, 130);
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 1);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(read_byte(c, addr), 4);
                c.reset();
            }
            // PHP Push Processor Status
            8 => {
                let flags = c.cpu.flags();
                php(c);
                // Startup processor status is on the stack
                assert_eq!(pull(c), flags | 0x10);
            }
            // ASL Shift Left A
            10 => {
                // Test cases
                //            | C | A   | z | n
                // val == 0   | 0 | 0   | 1 | 0
                // val <= 127 | 0 | 2*M | 0 | 0
                // val > 127  | 1 | 2*M | 0 | 0
                c.cpu.a = 0;
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 0);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(c.cpu.a, 0);
                c.reset();

                c.cpu.a = 50;
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 0);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(c.cpu.a, 100);
                c.reset();

                c.cpu.a = 130;
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 1);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(c.cpu.a, 4);
                c.reset();
            }
            // BPL Branch on Result Plus
            16 => {
                let cycles = c.cpu.cycles;
                let addr = 0x8000;
                bpl(c, addr);
                assert_eq!(c.cpu.pc, addr);
                assert_eq!(c.cpu.cycles, cycles + 1);
            }
            _ => eprintln!("Warning: opcode {} not covered", opcode),
        }
    }
}
