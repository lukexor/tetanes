use super::*;
use std::error::Error;

pub struct Console {
    pub cpu: CPU,
    pub apu: APU,
    pub ppu: PPU,
    pub cartridge: Cartridge,
    pub controller1: Controller,
    pub controller2: Controller,
    pub mapper: Box<Mapper>,
    pub ram: Vec<u8>,
}

impl Console {
    pub fn new(cartridge: Cartridge) -> Result<Self, Box<Error>> {
        let mapper = cartridge.get_mapper()?;
        Ok(Console {
            cpu: CPU::new(),
            apu: APU::new(),
            ppu: PPU::new(),
            cartridge,
            mapper,
            controller1: Controller::new(),
            controller2: Controller::new(),
            ram: vec![0; RAM_SIZE],
        })
    }

    pub fn reset(&mut self) {
        unimplemented!();
    }

    pub fn step_seconds(&mut self, seconds: f64) {
        let mut cycles = CPU_FREQUENCY as u64 * seconds as u64;
        while cycles > 0 {
            cycles -= self.step();
        }
    }

    pub fn step(&mut self) -> u64 {
        println!("{:?}", self.cpu);
        let cpu_cycles = if self.cpu.stall > 0 {
            self.cpu.stall -= 1;
            1
        } else {
            let start_cycles = self.cpu.cycles;
            match &self.cpu.interrupt {
                Interrupt::NMI => {
                    self.push16(self.cpu.pc);
                    self.php(InstructInfo::new());
                    self.cpu.pc = self.read16(0xFFFA);
                    self.cpu.i = 1;
                    self.cpu.cycles += 7;
                }
                Interrupt::IRQ => {
                    self.push16(self.cpu.pc);
                    self.php(InstructInfo::new());
                    self.cpu.pc = self.read16(0xFFFE);
                    self.cpu.i = 1;
                    self.cpu.cycles += 7;
                }
                _ => (),
            }
            self.cpu.interrupt = Interrupt::None;
            let opcode = self.read(self.cpu.pc);
            let instruction = &INSTRUCTIONS[opcode as usize];
            self.execute_instruction(instruction);
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

    /// Memory access

    pub fn read(&self, address: u16) -> u8 {
        match address {
            a if a < 0x2000 => self.ram[(address % 0x0800) as usize],
            a if a < 0x4000 => unimplemented!("self.ppu.read_reg(0x2000 + {} % 8)", address),
            a if a == 0x4014 => unimplemented!("self.ppu.read_reg(address)"),
            a if a == 0x4015 => unimplemented!("self.apu.read_reg(address)"),
            a if a == 0x4016 => unimplemented!("self.controller1.read()"),
            a if a == 0x4017 => unimplemented!("self.controller2.read()"),
            a if a < 0x6000 => unimplemented!("I/O registers"),
            a if a >= 0x6000 => unimplemented!("self.mapper.read_reg(address)"),
            _ => panic!("unhandled cpu memory read at address {:x}", address),
        }
    }

    pub fn read16(&self, address: u16) -> u16 {
        let lo = self.read(address) as u16;
        let hi = self.read(address + 1) as u16;
        hi << 8 | lo
    }

    pub fn write(&mut self, address: u16, val: u8) {
        match address {
            a if a < 0x2000 => self.ram[(address % 0x8000) as usize] = val,
            a if a < 0x4000 => unimplemented!("self.ppu.write_reg(0x2000 + address % 8, val)"),
            a if a < 0x4014 => unimplemented!("self.apu.write_reg(address, val)"),
            a if a == 0x4014 => unimplemented!("self.ppu.write_reg(address, val)"),
            a if a == 0x4015 => unimplemented!("self.apu.write_reg(address, val)"),
            a if a == 0x4016 => unimplemented!("self.controllers.write(address, val)"),
            a if a == 0x4017 => unimplemented!("self.apu.write_reg(address, val)"),
            a if a < 0x6000 => unimplemented!("I/O registeres"),
            a if a > 0x6000 => unimplemented!("self.mapper.write_reg(address, val)"),
            _ => panic!("unhandled cpu memory write at address {:x}", address),
        }
    }

    // read16bug emulates a 6502 bug that caused the low byte to wrap without
    // incrementing the high byte
    pub fn read16bug(&self, address: u16) -> u16 {
        let b = (address & 0xFF00) | ((address as u8) + 1) as u16;
        let lo = self.read(address) as u16;
        let hi = self.read(b) as u16;
        hi << 8 | lo
    }

    /// Stack Functions

    // Push byte to stack
    pub fn push(&mut self, val: u8) {
        self.write(0x100 | self.cpu.sp as u16, val);
        self.cpu.sp -= 1;
    }

    // Pull byte from stack
    pub fn pull(&mut self) -> u8 {
        self.cpu.sp += 1;
        self.read(0x100 | self.cpu.sp as u16)
    }

    // Push two bytes to stack
    pub fn push16(&mut self, val: u16) {
        let lo = (val & 0xFF) as u8;
        let hi = (val >> 8) as u8;
        self.push(hi);
        self.push(lo);
    }

    // Pull two bytes from stack
    pub fn pull16(&mut self) -> u16 {
        let lo = self.pull() as u16;
        let hi = self.pull() as u16;
        hi << 8 | lo
    }

    pub fn execute_instruction(&mut self, instruction: &Instruction) {
        let (address, page_crossed) = match &instruction.mode {
            AddrMode::Absolute => (self.read16(self.cpu.pc + 1), false),
            AddrMode::AbsoluteX => {
                let address = self.read16(self.cpu.pc + 1);
                let xaddress = address + self.cpu.x as u16;
                let page_crossed = CPU::pages_differ(address, xaddress);
                (xaddress, page_crossed)
            }
            AddrMode::AbsoluteY => {
                let address = self.read16(self.cpu.pc + 1);
                let yaddress = address + self.cpu.y as u16;
                let page_crossed = CPU::pages_differ(address, yaddress);
                (yaddress, page_crossed)
            }
            AddrMode::Accumulator => (0, false),
            AddrMode::Immediate => (self.cpu.pc + 1, false),
            AddrMode::Implied => (0, false),
            AddrMode::IndexedIndirect => (
                self.read16bug((self.read(self.cpu.pc + 1) + self.cpu.x) as u16),
                false,
            ),
            AddrMode::Indirect => (self.read16bug(self.read16(self.cpu.pc + 1)), false),
            AddrMode::IndirectIndexed => {
                let address = self.read16bug(self.read(self.cpu.pc + 1) as u16);
                let yaddress = address + self.cpu.y as u16;
                let page_crossed = CPU::pages_differ(address, yaddress);
                (yaddress, page_crossed)
            }
            AddrMode::Relative => {
                let mut offset = self.read(self.cpu.pc + 1) as u16;
                if offset >= 0x80 {
                    offset -= 0x100;
                }
                let address = self.cpu.pc + 2 + offset;
                (address, false)
            }
            AddrMode::ZeroPage => (self.read(self.cpu.pc + 1) as u16, false),
            AddrMode::ZeroPageX => (
                (self.read(self.cpu.pc + 1) + self.cpu.x) as u16 & 0xFF,
                false,
            ),
            AddrMode::ZeroPageY => (
                (self.read(self.cpu.pc + 1) + self.cpu.y) as u16 & 0xFF,
                false,
            ),
        };

        self.cpu.pc += instruction.size as u16;
        self.cpu.cycles += instruction.cycles as u64;
        if page_crossed {
            self.cpu.cycles += instruction.page_cycles as u64;
        }
        (*instruction.run)(
            self,
            InstructInfo {
                address,
                mode: instruction.mode,
            },
        );
    }

    /// Opcode functions

    pub fn brk(&mut self, info: InstructInfo) {}

    // Add with Carry
    pub fn adc(&mut self, info: InstructInfo) {
        let a = self.cpu.a;
        let b = self.read(info.address);
        let c = self.cpu.c;
        self.cpu.a = a + b + c;
        self.cpu.set_z(self.cpu.a);
        self.cpu.set_n(self.cpu.a);
        if (a + b + c) as i32 > 0xFF {
            self.cpu.c = 1;
        } else {
            self.cpu.c = 0;
        }
        if (a ^ b) & 0x80 == 0 && (a ^ self.cpu.a) & 0x80 != 0 {
            self.cpu.v = 1;
        } else {
            self.cpu.v = 0;
        }
    }

    pub fn php(&mut self, _info: InstructInfo) {
        let flags = self.cpu.flags();
        self.push(flags | 0x10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_console() {
        let rom = "roms/Zelda II - The Adventure of Link (USA).nes";
        let cartridge = Cartridge::new(rom).expect("valid cartridge");
        let console = Console::new(cartridge).expect("valid console");
    }
}
