use super::{
    apu::APU,
    cartridge::Cartridge,
    controller::Controller,
    cpu::{Interrupt, CPU},
    cpu_instructions::{execute, php, print_instruction},
    mapper::Mapper1,
    memory::{push16, read16, read_byte},
    ppu::PPU,
};
use std::{error::Error, path::PathBuf};

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
    pub fn new(rom: &PathBuf) -> Result<Self, Box<Error>> {
        let cartridge = Cartridge::new(rom)?;
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
                // Test cases
                // pages_differ
                // 0
                // 1
                let cycles = c.cpu.cycles;
                let addr = 0x0080;
                bpl(c, addr);
                assert_eq!(c.cpu.pc, addr);
                assert_eq!(c.cpu.cycles, cycles + 1);

                let cycles = c.cpu.cycles;
                let addr = 0xFF00;
                bpl(c, addr);
                assert_eq!(c.cpu.pc, addr);
                assert_eq!(c.cpu.cycles, cycles + 2);
            }
            // CLC Clear Carry Flag
            24 => {
                // Test cases
                // cpu.c = 0
                // cpu.c = 1
                c.cpu.c = 0;
                clc(c);
                assert_eq!(c.cpu.c, 0);

                c.cpu.c = 1;
                clc(c);
                assert_eq!(c.cpu.c, 0);
            }
            // Jump and Save return addr
            32 => {
                let pc = c.cpu.pc;
                jsr(c, addr);
                assert_eq!(u16::from(pull(c)), pc - 1);
                assert_eq!(c.cpu.pc, addr);
            }
            // "And" M with A
            33 | 37 | 41 | 45 | 49 | 53 | 57 | 61 => {
                // Test cases
                // M | A | M & A | z | n
                // 0 | 0 | 0     | 1 | 0
                // 1 | 0 | 0     | 1 | 0
                // 0 | 1 | 0     | 1 | 0
                // 1 | 1 | 1     | 0 | 0

                write(c, addr, 0);
                c.cpu.a = 0;
                and(c, addr);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 1);
                c.cpu.a = 0;
                and(c, addr);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 0);
                c.cpu.a = 1;
                and(c, addr);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 1);
                c.cpu.a = 1;
                and(c, addr);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                c.reset();
            }
            // BIT Test bits in M with A
            36 | 44 => {
                // Test cases
                // V | Z | N
                // 0 | 0 | 0
                // 1 | 0 | 0
                // 0 | 1 | 0
                // 1 | 1 | 0
                // 0 | 0 | 1
                // 1 | 0 | 1
                // 0 | 1 | 1
                // 1 | 1 | 1
                // bit(c, addr);
            }
            // 38 | 42 | 46 | 54 | 62 => rol(),
            // 40 => plp(c),
            // 48 => bmi(c, addr),
            // 56 => sec(c),
            // 64 => rti(),
            // 65 | 69 | 73 | 77 | 81 | 85 | 89 | 93 => eor(c, addr),
            // 70 | 74 | 78 | 86 | 94 => lsr(),
            // 72 => pha(c),
            // 76 | 108 => jmp(c, addr),
            // 80 => bvc(c, addr),
            // 88 => cli(c),
            // 96 => rts(c),
            // 97 | 101 | 105 | 109 | 113 | 117 | 121 | 125 => adc(c, addr),
            // 102 | 106 | 110 | 118 | 126 => ror(),
            // 104 => pla(c),
            // 112 => bvs(c, addr),
            // 120 => sei(c),
            // 129 | 133 | 141 | 145 | 149 | 153 | 157 => sta(c, addr),
            // 132 | 140 | 148 => sty(),
            // 134 | 142 | 150 => stx(c, addr),
            // 136 => dey(),
            // 138 => txa(),
            // 144 => bcc(c, addr),
            // 152 => tya(),
            // 154 => txs(c),
            // 160 | 164 | 172 | 180 | 188 => ldy(c, addr),
            // 161 | 165 | 169 | 173 | 177 | 181 | 185 | 189 => lda(c, addr),
            // 162 | 166 | 174 | 182 | 190 => ldx(c, addr),
            // 168 => tay(),
            // 170 => tax(),
            // 176 => bcs(c, addr),
            // 184 => clv(c),
            // 186 => tsx(),
            // 192 | 196 | 204 => cpy(),
            // 193 | 197 | 201 | 205 | 209 | 213 | 217 | 221 => cmp(c, addr),
            // 198 | 206 | 214 | 222 => dec(),
            // 200 => iny(),
            // 202 => dex(),
            // 208 => bne(c, addr),
            // 216 => cld(c),
            // 224 | 228 | 236 => cpx(),
            // 225 | 229 | 233 | 235 | 237 | 241 | 245 | 249 | 253 => sbc(),
            // 230 | 238 | 246 | 254 => inc(),
            // 232 => inx(),
            // 240 => beq(c, addr),
            // 248 => sed(c),
            _ => eprintln!("Warning: opcode {} not covered", opcode),
        }
    }
}
