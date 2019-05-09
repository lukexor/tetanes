//! A 6502 NES Central Processing Unit
//!
//! http://wiki.nesdev.com/w/index.php/CPU

use crate::console::cartridge::Board;
use crate::console::memory::{Addr, Byte, CpuMemMap, Memory, Word};
use std::fmt;
use std::sync::{Arc, Mutex};

pub type Cycle = u64;

// 1.79 MHz (~559 ns/cycle) - May want to use 1_786_830 for a stable 60 FPS
// const CPU_CLOCK_FREQ: Frequency = 1_789_773.0;
const NMI_ADDR: Addr = 0xFFFA;
const IRQ_ADDR: Addr = 0xFFFE;
const RESET_ADDR: Addr = 0xFFFC;
const RESET_SP: Byte = 0xFD;
const RESET_STATUS: Byte = 0x24; // 0010 0100 - Unused and Interrupt Disable set

// Status Registers
// http://wiki.nesdev.com/w/index.php/Status_flags
// 7654 3210
// NVUB DIZC
// |||| ||||
// |||| |||+- Carry
// |||| ||+-- Zero
// |||| |+--- Interrupt Disable
// |||| +---- Decimal - Not used in the NES but still has to function
// |||+------ Break Flag - 1 when pushed to stack from PHP/BRK, 0 from IRQ/NMI
// ||+------- Unused - always set to 1 when pushed to stack
// |+-------- Overflow
// +--------- Negative
const CARRY_FLAG: Byte = 1;
const ZERO_FLAG: Byte = 1 << 1;
const INTERRUPTD_FLAG: Byte = 1 << 2;
const DECIMAL_FLAG: Byte = 1 << 3;
const BREAK_FLAG: Byte = 1 << 4;
const UNUSED_FLAG: Byte = 1 << 5;
const OVERFLOW_FLAG: Byte = 1 << 6;
const NEGATIVE_FLAG: Byte = 1 << 7;

#[cfg(test)]
const CPU_TRACE_LOG: &str = "logs/cpu.log";

/// The Central Processing Unit
pub struct Cpu {
    cycles: Cycle, // number of cycles
    stall: Cycle,  // number of cycles to stall
    pc: Addr,      // program counter
    sp: Byte,      // stack pointer - stack is at $0100-$01FF
    acc: Byte,     // accumulator
    x: Byte,       // x register
    y: Byte,       // y register
    status: Byte,
    mem: CpuMemMap,
    #[cfg(test)]
    trace: bool,
    #[cfg(test)]
    pub oplog: String,
}

impl Cpu {
    pub fn init(mem: CpuMemMap) -> Self {
        Self {
            cycles: 0,
            stall: 0,
            pc: 0,
            sp: 0,
            acc: 0,
            x: 0,
            y: 0,
            status: 0,
            mem,
            #[cfg(test)]
            trace: false,
            #[cfg(test)]
            oplog: String::with_capacity(9000 * 60),
        }
    }

    /// Resets the CPU status to a known state.
    ///
    /// Updates the PC, SP, and Status values to defined constants.
    ///
    /// These operations take the CPU 7 cycles.
    pub fn reset(&mut self) {
        self.pc = self.mem.readw(RESET_ADDR);
        self.sp = RESET_SP;
        self.status = RESET_STATUS;
        self.cycles = 7;
    }

    /// Steps the CPU exactly one `tick`
    ///
    /// Returns the number of cycles the CPU took to execute.
    pub fn step(&mut self) {
        if self.stall > 0 {
            self.stall -= 1;
        } else {
            let start_cycles = self.cycles;
            let opcode = self.mem.readb(self.pc);
            self.execute(opcode);
            self.mem.ppu.step(self.cycles);
        }
    }

    /// Sends an IRQ Interrupt to the CPU
    ///
    /// http://wiki.nesdev.com/w/index.php/IRQ
    pub fn irq(&mut self) {
        if self.interrupt_disable() {
            return;
        }
        self.push_stackw(self.pc);
        self.push_stackb((self.status | UNUSED_FLAG) & !BREAK_FLAG);
        self.status |= INTERRUPTD_FLAG;
        self.pc = self.mem.readw(IRQ_ADDR);
        self.cycles = self.cycles.wrapping_add(7);
    }

    /// Sends a NMI Interrupt to the CPU
    ///
    /// http://wiki.nesdev.com/w/index.php/NMI
    pub fn nmi(&mut self) {
        self.push_stackw(self.pc);
        self.push_stackb((self.status | UNUSED_FLAG) & !BREAK_FLAG);
        self.pc = self.mem.readw(NMI_ADDR);
        self.cycles = self.cycles.wrapping_add(7);
    }

    pub fn set_board(&mut self, board: Arc<Mutex<Board>>) {
        self.mem.set_board(board);
    }

    // Executes a single CPU instruction
    fn execute(&mut self, opcode: Byte) {
        let (op, addr_mode, cycles, page_cycles) = OPCODES[opcode as usize];
        let (val, target, num_args, page_crossed) =
            self.decode_addr_mode(addr_mode, self.pc.wrapping_add(1), op);
        #[cfg(test)]
        {
            if self.trace {
                self.print_instruction(op, opcode, 1 + num_args);
            }
        }
        self.pc = self.pc.wrapping_add(1 + Word::from(num_args));
        self.cycles += cycles;
        if page_crossed {
            self.cycles += page_cycles;
        }

        let val = val as Byte;
        match op {
            ADC => self.adc(val),
            AND => self.and(val),
            ASL => {
                let val = self.read_target(target);
                let res = self.asl(val);
                self.write_target(target, res);
            }
            BCC => self.bcc(val),
            BCS => self.bcs(val),
            BEQ => self.beq(val),
            BIT => self.bit(val),
            BMI => self.bmi(val),
            BNE => self.bne(val),
            BPL => self.bpl(val),
            // Break Interrupt
            BRK => self.brk(),
            BVC => self.bvc(val),
            BVS => self.bvs(val),
            CLC => self.clc(),
            CLD => self.cld(),
            CLI => self.cli(),
            CLV => self.clv(),
            CMP => self.cmp(val),
            CPX => self.cpx(val),
            CPY => self.cpy(val),
            DEC => {
                let val = self.read_target(target);
                let res = self.dec(val);
                self.write_target(target, res);
            }
            DEX => self.dex(),
            DEY => self.dey(),
            EOR => self.eor(val),
            INC => {
                let val = self.read_target(target);
                let res = self.inc(val);
                self.write_target(target, res);
            }
            INX => self.inx(),
            INY => self.iny(),
            JMP => self.jmp(target.unwrap()),
            JSR => self.jsr(target.unwrap()),
            LAX => self.lax(val),
            LDA => self.lda(val),
            LDX => self.ldx(val),
            LDY => self.ldy(val),
            LSR => {
                let val = self.read_target(target);
                let res = self.lsr(val);
                self.write_target(target, res);
            }
            NOP => self.nop(),
            ORA => self.ora(val),
            PHA => self.pha(),
            PHP => self.php(),
            PLA => self.pla(),
            PLP => self.plp(),
            ROL => {
                let val = self.read_target(target);
                let res = self.rol(val);
                self.write_target(target, res);
            }
            ROR => {
                let val = self.read_target(target);
                let res = self.ror(val);
                self.write_target(target, res);
            }
            RTI => self.rti(),
            RTS => self.rts(),
            SBC => self.sbc(val),
            SEC => self.sec(),
            SED => self.sed(),
            SEI => self.sei(),
            STA => self.write_target(target, self.acc),
            STX => self.write_target(target, self.x),
            STY => self.write_target(target, self.y),
            TAX => self.tax(),
            TAY => self.tay(),
            TSX => self.tsx(),
            TXA => self.txa(),
            TXS => self.txs(),
            TYA => self.tya(),
            KIL => panic!("KIL encountered"),
            SAX => {
                let res = self.sax();
                self.write_target(target, res);
            }
            DCP => {
                let val = self.read_target(target);
                let res = self.dcp(val);
                self.write_target(target, res);
            }
            ISB => {
                let val = self.read_target(target);
                let res = self.isb(val);
                self.write_target(target, res);
            }
            RLA => {
                let val = self.read_target(target);
                let res = self.rla(val);
                self.write_target(target, res);
            }
            RRA => {
                let val = self.read_target(target);
                let res = self.rra(val);
                self.write_target(target, res);
            }
            SLO => {
                let val = self.read_target(target);
                let res = self.slo(val);
                self.write_target(target, res);
            }
            SRE => {
                let val = self.read_target(target);
                let res = self.sre(val);
                self.write_target(target, res);
            }
            _ => panic!("unhandled operation {:?}", op),
        };
    }

    // Getters/Setters

    // Updates the accumulator register
    fn update_acc(&mut self) {
        self.set_result_flags(self.acc);
    }

    // Sets the zero and negative registers appropriately
    fn set_result_flags(&mut self, val: Byte) {
        match val {
            0 => {
                self.set_zero(true);
                self.set_negative(false);
            }
            v if is_negative(v) => {
                self.set_zero(false);
                self.set_negative(true);
            }
            _ => {
                self.set_zero(false);
                self.set_negative(false);
            }
        }
    }

    #[cfg(test)]
    // Used for testing to manually set the PC to a known value
    pub fn set_pc(&mut self, addr: Addr) {
        self.pc = addr;
    }

    #[cfg(test)]
    // Used for testing to print a log of CPU instructions
    pub fn set_trace(&mut self, val: bool) {
        use std::fs;
        self.trace = val;
        fs::write(CPU_TRACE_LOG, "").expect("cpu trace log");
    }

    // Status Register functions

    fn carry(&self) -> bool {
        (self.status & CARRY_FLAG) == CARRY_FLAG
    }
    fn set_carry(&mut self, val: bool) {
        if val {
            self.status |= CARRY_FLAG;
        } else {
            self.status &= !CARRY_FLAG;
        }
    }

    fn zero(&self) -> bool {
        (self.status & ZERO_FLAG) == ZERO_FLAG
    }
    fn set_zero(&mut self, val: bool) {
        if val {
            self.status |= ZERO_FLAG;
        } else {
            self.status &= !ZERO_FLAG;
        }
    }

    fn interrupt_disable(&self) -> bool {
        (self.status & INTERRUPTD_FLAG) == INTERRUPTD_FLAG
    }
    fn set_interrupt_disable(&mut self, val: bool) {
        if val {
            self.status |= INTERRUPTD_FLAG;
        } else {
            self.status &= !INTERRUPTD_FLAG;
        }
    }

    fn set_decimal(&mut self, val: bool) {
        if val {
            self.status |= DECIMAL_FLAG;
        } else {
            self.status &= !DECIMAL_FLAG;
        }
    }

    fn overflow(&self) -> bool {
        (self.status & OVERFLOW_FLAG) == OVERFLOW_FLAG
    }
    fn set_overflow(&mut self, val: bool) {
        if val {
            self.status |= OVERFLOW_FLAG;
        } else {
            self.status &= !OVERFLOW_FLAG;
        }
    }

    fn negative(&self) -> bool {
        (self.status & NEGATIVE_FLAG) == NEGATIVE_FLAG
    }
    fn set_negative(&mut self, val: bool) {
        if val {
            self.status |= NEGATIVE_FLAG;
        } else {
            self.status &= !NEGATIVE_FLAG;
        }
    }

    // Stack Functions

    // Push a byte to the stack
    fn push_stackb(&mut self, val: Byte) {
        self.mem.writeb(0x100 | Addr::from(self.sp), val);
        self.sp = self.sp.wrapping_sub(1);
    }

    // Pull a byte from the stack
    fn pop_stackb(&mut self) -> Byte {
        self.sp = self.sp.wrapping_add(1);
        self.mem.readb(0x100 | Addr::from(self.sp))
    }

    // Push a word (two bytes) to the stack
    fn push_stackw(&mut self, val: u16) {
        let lo = (val & 0xFF) as Byte;
        let hi = (val >> 8) as Byte;
        self.push_stackb(hi);
        self.push_stackb(lo);
    }

    // Pull a word (two bytes) from the stack
    fn pop_stackw(&mut self) -> Word {
        let lo = Word::from(self.pop_stackb());
        let hi = Word::from(self.pop_stackb());
        hi << 8 | lo
    }

    // Decodes the addressing mode of the instruction and returns the target value, address (if
    // there is one), number of bytes used after the opcode, and whether it crossed a page
    // boundary
    fn decode_addr_mode(
        &mut self,
        mode: AddrMode,
        addr: Addr,
        op: Operation,
    ) -> (Word, Option<Addr>, u8, bool) {
        // Whether to read from memory or not
        // ST* opcodes only require the address not the value
        // so this saves unnecessary memory reads
        let read = match op {
            STA | STX | STY => false,
            _ => true,
        };
        match mode {
            Immediate => {
                let val = if read {
                    Addr::from(self.mem.readb(addr))
                } else {
                    0
                };
                (val, Some(addr), 1, false)
            }
            ZeroPage => {
                let addr = Addr::from(self.mem.readb(addr));
                let val = if read {
                    Word::from(self.mem.readb(addr))
                } else {
                    0
                };
                (val, Some(addr), 1, false)
            }
            ZeroPageX => {
                let addr = Addr::from(self.mem.readb(addr).wrapping_add(self.x));
                let val = if read {
                    Word::from(self.mem.readb(addr))
                } else {
                    0
                };
                (val, Some(addr), 1, false)
            }
            ZeroPageY => {
                let addr = Addr::from(self.mem.readb(addr).wrapping_add(self.y));
                let val = if read {
                    Word::from(self.mem.readb(addr))
                } else {
                    0
                };
                (val, Some(addr), 1, false)
            }
            Absolute => {
                let addr = self.mem.readw(addr);
                let val = if read {
                    Word::from(self.mem.readb(addr))
                } else {
                    0
                };
                (val, Some(addr), 2, false)
            }
            AbsoluteX => {
                let addr0 = self.mem.readw(addr);
                let addr = addr0.wrapping_add(Addr::from(self.x));
                let val = if read {
                    Word::from(self.mem.readb(addr))
                } else {
                    0
                };
                let page_crossed = Cpu::pages_differ(addr0, addr);
                (val, Some(addr), 2, page_crossed)
            }
            AbsoluteY => {
                let addr0 = self.mem.readw(addr);
                let addr = addr0.wrapping_add(Addr::from(self.y));
                let val = if read {
                    Word::from(self.mem.readb(addr))
                } else {
                    0
                };
                let page_crossed = Cpu::pages_differ(addr0, addr);
                (val, Some(addr), 2, page_crossed)
            }
            Indirect => {
                let addr0 = self.mem.readw(addr);
                let addr = self.mem.readw_pagewrap(addr0);
                (0, Some(addr), 2, false)
            }
            IndirectX => {
                let addr_zp = self.mem.readb(addr).wrapping_add(self.x);
                let addr = self.mem.readw_zp(addr_zp);
                let val = if read {
                    Word::from(self.mem.readb(addr))
                } else {
                    0
                };
                (val, Some(addr), 1, false)
            }
            IndirectY => {
                let addr_zp = self.mem.readb(addr);
                let addr_zp = self.mem.readw_zp(addr_zp);
                let addr = addr_zp.wrapping_add(Addr::from(self.y));
                let val = if read {
                    Word::from(self.mem.readb(addr))
                } else {
                    0
                };
                let page_crossed = Cpu::pages_differ(addr_zp, addr);
                (val, Some(addr), 1, page_crossed)
            }
            Relative => {
                let val = if read {
                    Word::from(self.mem.readb(addr))
                } else {
                    0
                };
                (val, Some(addr), 1, false)
            }
            Accumulator => (Word::from(self.acc), None, 0, false),
            Implied => (0, None, 0, false),
        }
    }

    // Reads from either a target address or the accumulator register.
    //
    // target is either Some(Addr) or None based on the addressing mode
    fn read_target(&mut self, target: Option<u16>) -> Byte {
        match target {
            None => self.acc,
            Some(addr) => self.mem.readb(addr),
        }
    }

    // Reads from either a target address or the accumulator register.
    //
    // target is either Some(Addr) or None based on the addressing mode
    fn write_target(&mut self, target: Option<u16>, val: Byte) {
        match target {
            None => {
                self.acc = val;
            }
            Some(addr) => self.mem.writeb(addr, val),
        }
    }

    #[cfg(test)]
    // Print the current instruction and status
    fn print_instruction(&mut self, op: Operation, opcode: Byte, num_args: u8) {
        let word1 = if num_args < 2 {
            "  ".to_string()
        } else {
            format!("{:02X}", self.mem.readb(self.pc.wrapping_add(1)))
        };
        let word2 = if num_args < 3 {
            "  ".to_string()
        } else {
            format!("{:02X}", self.mem.readb(self.pc.wrapping_add(2)))
        };
        let asterisk = match op {
            NOP if opcode != 0xEA => "*",
            SBC if opcode == 0xEB => "*",
            DCP | ISB | LAX | RLA | RRA | SAX | SLO | SRE => "*",
            _ => " ",
        };
        let opstr = format!(
            "{:04X}  {:02X} {} {} {}{:29?} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{}\n",
            self.pc,
            opcode,
            word1,
            word2,
            asterisk,
            op,
            self.acc,
            self.x,
            self.y,
            self.status,
            self.sp,
            self.cycles,
        );
        self.oplog.push_str(&opstr);
    }

    // Determines if address a and address b are on different pages
    fn pages_differ(a: u16, b: u16) -> bool {
        a & 0xFF00 != b & 0xFF00
    }
}

impl fmt::Debug for Cpu {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "CPU {{ {:04X} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{} }}",
            self.pc, self.acc, self.x, self.y, self.status, self.sp, self.cycles,
        )
    }
}

#[rustfmt::skip]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
// List of all CPU official and unofficial operations
// http://wiki.nesdev.com/w/index.php/6502_instructions
enum Operation {
    ADC, AND, ASL, BCC, BCS, BEQ, BIT, BMI, BNE, BPL, BRK, BVC, BVS, CLC, CLD, CLI, CLV, CMP, CPX,
    CPY, DEC, DEX, DEY, EOR, INC, INX, INY, JMP, JSR, LDA, LDX, LDY, LSR, NOP, ORA, PHA, PHP, PLA,
    PLP, ROL, ROR, RTI, RTS, SBC, SEC, SED, SEI, STA, STX, STY, TAX, TAY, TSX, TXA, TXS, TYA,
    // "Unofficial" opcodes
    KIL, ISB, DCP, AXS, LAS, LAX, AHX, SAX, XAA, SHX, RRA, TAS, SHY, ARR, SRE, ALR, RLA, ANC, SLO,
}

#[rustfmt::skip]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
// List of all addressing modes
enum AddrMode {
    Immediate,
    ZeroPage, ZeroPageX, ZeroPageY,
    Absolute, AbsoluteX, AbsoluteY,
    Indirect, IndirectX, IndirectY,
    Relative,
    Accumulator,
    Implied,
}

use AddrMode::*;
use Operation::*;

type CycleCount = u64;
const IMM: AddrMode = Immediate;
const ZRP: AddrMode = ZeroPage;
const ZRX: AddrMode = ZeroPageX;
const ZRY: AddrMode = ZeroPageY;
const ABS: AddrMode = Absolute;
const ABX: AddrMode = AbsoluteX;
const ABY: AddrMode = AbsoluteY;
const IND: AddrMode = Indirect;
const IDX: AddrMode = IndirectX;
const IDY: AddrMode = IndirectY;
const REL: AddrMode = Relative;
const ACC: AddrMode = Accumulator;
const IMP: AddrMode = Implied;

// CPU Opcodes
//
// (Operation, Addressing Mode, Clock Cycles, Extra Cycles if page boundary is crossed)
#[rustfmt::skip]
const OPCODES: [(Operation, AddrMode, CycleCount, CycleCount); 256] = [
    (BRK, IMM, 7, 0), (ORA, IDX, 6, 0), (KIL, IMP, 0, 0), (SLO, IDX, 8, 0), // $00 - $03
    (NOP, ZRP, 3, 0), (ORA, ZRP, 3, 0), (ASL, ZRP, 5, 0), (SLO, ZRP, 5, 0), // $04 - $07
    (PHP, IMP, 3, 0), (ORA, IMM, 2, 0), (ASL, ACC, 2, 0), (ANC, IMM, 2, 0), // $08 - $0B
    (NOP, ABS, 4, 0), (ORA, ABS, 4, 0), (ASL, ABS, 6, 0), (SLO, ABS, 6, 0), // $0C - $0F
    (BPL, REL, 2, 1), (ORA, IDY, 5, 1), (KIL, IMP, 0, 0), (SLO, IDY, 8, 0), // $10 - $13
    (NOP, ZRX, 4, 0), (ORA, ZRX, 4, 0), (ASL, ZRX, 6, 0), (SLO, ZRX, 6, 0), // $14 - $17
    (CLC, IMP, 2, 0), (ORA, ABY, 4, 1), (NOP, IMP, 2, 0), (SLO, ABY, 7, 0), // $18 - $1B
    (NOP, ABX, 4, 1), (ORA, ABX, 4, 1), (ASL, ABX, 7, 0), (SLO, ABX, 7, 0), // $1C - $1F
    (JSR, ABS, 6, 0), (AND, IDX, 6, 0), (KIL, IMP, 0, 0), (RLA, IDX, 8, 0), // $20 - $23
    (BIT, ZRP, 3, 0), (AND, ZRP, 3, 0), (ROL, ZRP, 5, 0), (RLA, ZRP, 5, 0), // $24 - $27
    (PLP, IMP, 4, 0), (AND, IMM, 2, 0), (ROL, ACC, 2, 0), (ANC, IMM, 2, 0), // $28 - $2B
    (BIT, ABS, 4, 0), (AND, ABS, 4, 0), (ROL, ABS, 6, 0), (RLA, ABS, 6, 0), // $2C - $2F
    (BMI, REL, 2, 1), (AND, IDY, 5, 1), (KIL, IMP, 0, 0), (RLA, IDY, 8, 0), // $30 - $33
    (NOP, ZRX, 4, 0), (AND, ZRX, 4, 0), (ROL, ZRX, 6, 0), (RLA, ZRX, 6, 0), // $34 - $37
    (SEC, IMP, 2, 0), (AND, ABY, 4, 1), (NOP, IMP, 2, 0), (RLA, ABY, 7, 0), // $38 - $3B
    (NOP, ABX, 4, 1), (AND, ABX, 4, 1), (ROL, ABX, 7, 0), (RLA, ABX, 7, 0), // $3C - $3F
    (RTI, IMP, 6, 0), (EOR, IDX, 6, 0), (KIL, IMP, 0, 0), (SRE, IDX, 8, 0), // $40 - $43
    (NOP, ZRP, 3, 0), (EOR, ZRP, 3, 0), (LSR, ZRP, 5, 0), (SRE, ZRP, 5, 0), // $44 - $47
    (PHA, IMP, 3, 0), (EOR, IMM, 2, 0), (LSR, ACC, 2, 0), (ALR, IMM, 2, 0), // $48 - $4B
    (JMP, ABS, 3, 0), (EOR, ABS, 4, 0), (LSR, ABS, 6, 0), (SRE, ABS, 6, 0), // $4C - $4F
    (BVC, REL, 2, 1), (EOR, IDY, 5, 1), (KIL, IMP, 0, 0), (SRE, IDY, 8, 0), // $50 - $53
    (NOP, ZRX, 4, 0), (EOR, ZRX, 4, 0), (LSR, ZRX, 6, 0), (SRE, ZRX, 6, 0), // $54 - $57
    (CLI, IMP, 2, 0), (EOR, ABY, 4, 1), (NOP, IMP, 2, 0), (SRE, ABY, 7, 0), // $58 - $5B
    (NOP, ABX, 4, 1), (EOR, ABX, 4, 1), (LSR, ABX, 7, 0), (SRE, ABX, 7, 0), // $5C - $5F
    (RTS, IMP, 6, 0), (ADC, IDX, 6, 0), (KIL, IMP, 0, 0), (RRA, IDX, 8, 0), // $60 - $63
    (NOP, ZRP, 3, 0), (ADC, ZRP, 3, 0), (ROR, ZRP, 5, 0), (RRA, ZRP, 5, 0), // $64 - $67
    (PLA, IMP, 4, 0), (ADC, IMM, 2, 0), (ROR, ACC, 2, 0), (ARR, IMM, 2, 0), // $68 - $6B
    (JMP, IND, 5, 0), (ADC, ABS, 4, 0), (ROR, ABS, 6, 0), (RRA, ABS, 6, 0), // $6C - $6F
    (BVS, REL, 2, 1), (ADC, IDY, 5, 1), (KIL, IMP, 0, 0), (RRA, IDY, 8, 0), // $70 - $73
    (NOP, ZRX, 4, 0), (ADC, ZRX, 4, 0), (ROR, ZRX, 6, 0), (RRA, ZRX, 6, 0), // $74 - $77
    (SEI, IMP, 2, 0), (ADC, ABY, 4, 1), (NOP, IMP, 2, 0), (RRA, ABY, 7, 0), // $78 - $7B
    (NOP, ABX, 4, 1), (ADC, ABX, 4, 1), (ROR, ABX, 7, 0), (RRA, ABX, 7, 0), // $7C - $7F
    (NOP, IMM, 2, 0), (STA, IDX, 6, 0), (NOP, IMM, 2, 0), (SAX, IDX, 6, 0), // $80 - $83
    (STY, ZRP, 3, 0), (STA, ZRP, 3, 0), (STX, ZRP, 3, 0), (SAX, ZRP, 3, 0), // $84 - $87
    (DEY, IMP, 2, 0), (NOP, IMM, 2, 0), (TXA, IMP, 2, 0), (XAA, IMM, 2, 1), // $88 - $8B
    (STY, ABS, 4, 0), (STA, ABS, 4, 0), (STX, ABS, 4, 0), (SAX, ABS, 4, 0), // $8C - $8F
    (BCC, REL, 2, 1), (STA, IDY, 6, 0), (KIL, IMP, 0, 0), (AHX, IDY, 6, 0), // $90 - $93
    (STY, ZRX, 4, 0), (STA, ZRX, 4, 0), (STX, ZRY, 4, 0), (SAX, ZRY, 4, 0), // $94 - $97
    (TYA, IMP, 2, 0), (STA, ABY, 5, 0), (TXS, IMP, 2, 0), (TAS, ABY, 5, 0), // $98 - $9B
    (SHY, ABX, 5, 0), (STA, ABX, 5, 0), (SHX, ABY, 5, 0), (AHX, ABY, 5, 0), // $9C - $9F
    (LDY, IMM, 2, 0), (LDA, IDX, 6, 0), (LDX, IMM, 2, 0), (LAX, IDX, 6, 0), // $A0 - $A3
    (LDY, ZRP, 3, 0), (LDA, ZRP, 3, 0), (LDX, ZRP, 3, 0), (LAX, ZRP, 3, 0), // $A4 - $A7
    (TAY, IMP, 2, 0), (LDA, IMM, 2, 0), (TAX, IMP, 2, 0), (LAX, IMM, 2, 0), // $A8 - $AB
    (LDY, ABS, 4, 0), (LDA, ABS, 4, 0), (LDX, ABS, 4, 0), (LAX, ABS, 4, 0), // $AC - $AF
    (BCS, REL, 2, 1), (LDA, IDY, 5, 1), (KIL, IMP, 0, 0), (LAX, IDY, 5, 1), // $B0 - $B3
    (LDY, ZRX, 4, 0), (LDA, ZRX, 4, 0), (LDX, ZRY, 4, 0), (LAX, ZRY, 4, 0), // $B4 - $B7
    (CLV, IMP, 2, 0), (LDA, ABY, 4, 1), (TSX, IMP, 2, 0), (LAS, ABY, 4, 1), // $B8 - $BB
    (LDY, ABX, 4, 1), (LDA, ABX, 4, 1), (LDX, ABY, 4, 1), (LAX, ABY, 4, 1), // $BC - $BF
    (CPY, IMM, 2, 0), (CMP, IDX, 6, 0), (NOP, IMM, 2, 0), (DCP, IDX, 8, 0), // $C0 - $C3
    (CPY, ZRP, 3, 0), (CMP, ZRP, 3, 0), (DEC, ZRP, 5, 0), (DCP, ZRP, 5, 0), // $C4 - $C7
    (INY, IMP, 2, 0), (CMP, IMM, 2, 0), (DEX, IMP, 2, 0), (AXS, IMM, 2, 0), // $C8 - $CB
    (CPY, ABS, 4, 0), (CMP, ABS, 4, 0), (DEC, ABS, 6, 0), (DCP, ABS, 6, 0), // $CC - $CF
    (BNE, REL, 2, 1), (CMP, IDY, 5, 1), (KIL, IMP, 0, 0), (DCP, IDY, 8, 0), // $D0 - $D3
    (NOP, ZRX, 4, 0), (CMP, ZRX, 4, 0), (DEC, ZRX, 6, 0), (DCP, ZRX, 6, 0), // $D4 - $D7
    (CLD, IMP, 2, 0), (CMP, ABY, 4, 1), (NOP, IMP, 2, 0), (DCP, ABY, 7, 0), // $D8 - $DB
    (NOP, ABX, 4, 1), (CMP, ABX, 4, 1), (DEC, ABX, 7, 0), (DCP, ABX, 7, 0), // $DC - $DF
    (CPX, IMM, 2, 0), (SBC, IDX, 6, 0), (NOP, IMM, 2, 0), (ISB, IDX, 8, 0), // $E0 - $E3
    (CPX, ZRP, 3, 0), (SBC, ZRP, 3, 0), (INC, ZRP, 5, 0), (ISB, ZRP, 5, 0), // $E4 - $E7
    (INX, IMP, 2, 0), (SBC, IMM, 2, 0), (NOP, IMP, 2, 0), (SBC, IMM, 2, 0), // $E8 - $EB
    (CPX, ABS, 4, 0), (SBC, ABS, 4, 0), (INC, ABS, 6, 0), (ISB, ABS, 6, 0), // $EC - $EF
    (BEQ, REL, 2, 1), (SBC, IDY, 5, 1), (KIL, IMP, 0, 0), (ISB, IDY, 8, 0), // $F0 - $F3
    (NOP, ZRX, 4, 0), (SBC, ZRX, 4, 0), (INC, ZRX, 6, 0), (ISB, ZRX, 6, 0), // $F4 - $F7
    (SED, IMP, 2, 0), (SBC, ABY, 4, 1), (NOP, IMP, 2, 0), (ISB, ABY, 7, 0), // $F8 - $FB
    (NOP, ABX, 4, 1), (SBC, ABX, 4, 1), (INC, ABX, 7, 0), (ISB, ABX, 7, 0), // $FC - $FF
];

impl Cpu {
    // Storage opcodes

    // LDA: Load A with M
    fn lda(&mut self, val: Byte) {
        self.acc = val;
        self.update_acc();
    }

    // LDX: Load X with M
    fn ldx(&mut self, val: Byte) {
        self.x = val;
        self.set_result_flags(val);
    }

    // LDY: Load Y with M
    fn ldy(&mut self, val: Byte) {
        self.y = val;
        self.set_result_flags(val);
    }

    // TAX: Transfer A to X
    fn tax(&mut self) {
        self.x = self.acc;
        self.set_result_flags(self.x);
    }

    // TAY: Transfer A to Y
    fn tay(&mut self) {
        self.y = self.acc;
        self.set_result_flags(self.y);
    }

    // TSX: Transfer Stack Pointer to X
    fn tsx(&mut self) {
        self.x = self.sp;
        self.set_result_flags(self.x);
    }

    // TXA: Transfer X to A
    fn txa(&mut self) {
        self.acc = self.x;
        self.update_acc();
    }

    // TXS: Transfer X to Stack Pointer
    fn txs(&mut self) {
        self.sp = self.x;
    }

    // TYA: Transfer Y to A
    fn tya(&mut self) {
        self.acc = self.y;
        self.update_acc();
    }

    // Arithmetic opcode

    // ADC: Add M to A with Carry
    fn adc(&mut self, val: Byte) {
        let a = self.acc;
        let (x1, o1) = val.overflowing_add(a);
        let (x2, o2) = x1.overflowing_add(self.carry() as Byte);
        self.acc = x2;
        self.set_carry(o1 | o2);
        self.set_overflow((a ^ val) & 0x80 == 0 && (a ^ self.acc) & 0x80 != 0);
        self.update_acc();
    }

    // SBC: Subtract M from A with Carry
    fn sbc(&mut self, val: Byte) {
        let a = self.acc;
        let (x1, o1) = a.overflowing_sub(val);
        let (x2, o2) = x1.overflowing_sub(1 - self.carry() as Byte);
        self.acc = x2;
        self.set_carry(!(o1 | o2));
        self.set_overflow((a ^ val) & 0x80 != 0 && (a ^ self.acc) & 0x80 != 0);
        self.update_acc();
    }

    // DEC: Decrement M by One
    fn dec(&mut self, val: Byte) -> Byte {
        let val = val.wrapping_sub(1);
        self.set_result_flags(val);
        val
    }

    // DEX: Decrement X by One
    fn dex(&mut self) {
        let x = self.x;
        self.x = self.dec(x);
    }

    // DEY: Decrement Y by One
    fn dey(&mut self) {
        let y = self.y;
        self.y = self.dec(y);
    }

    // INC: Increment M by One
    fn inc(&mut self, val: Byte) -> Byte {
        let val = val.wrapping_add(1);
        self.set_result_flags(val);
        val
    }

    // INX: Increment X by One
    fn inx(&mut self) {
        let x = self.x;
        self.x = self.inc(x);
    }

    // INY: Increment Y by One
    fn iny(&mut self) {
        let y = self.y;
        self.y = self.inc(y);
    }

    // Bitwise opcodes

    // AND: "And" M with A
    fn and(&mut self, val: Byte) {
        self.acc &= val;
        self.update_acc();
    }

    // ASL: Shift Left One Bit (M or A)
    fn asl(&mut self, val: Byte) -> Byte {
        self.set_carry((val >> 7) & 1 > 0);
        let val = val.wrapping_shl(1);
        self.set_result_flags(val);
        val
    }

    // BIT: Test Bits in M with A
    fn bit(&mut self, val: Byte) {
        self.set_overflow((val >> 6) & 1 > 0);
        self.set_zero((val & self.acc) == 0);
        self.set_negative(is_negative(val));
    }

    // EOR: "Exclusive-Or" M with A
    fn eor(&mut self, val: Byte) {
        self.acc ^= val;
        self.update_acc();
    }

    // LSR: Shift Right One Bit (M or A)
    fn lsr(&mut self, val: Byte) -> Byte {
        self.set_carry(val & 1 > 0);
        let val = val.wrapping_shr(1);
        self.set_result_flags(val);
        val
    }

    // ORA: "OR" M with A
    fn ora(&mut self, val: Byte) {
        self.acc |= val;
        self.update_acc();
    }

    // ROL: Rotate One Bit Left (M or A)
    fn rol(&mut self, val: Byte) -> Byte {
        let old_c = self.carry() as Byte;
        self.set_carry((val >> 7) & 1 > 0);
        let val = (val << 1) | old_c;
        self.set_result_flags(val);
        val
    }

    // ROR: Rotate One Bit Right (M or A)
    fn ror(&mut self, val: Byte) -> Byte {
        let mut ret = val.rotate_right(1);
        if self.carry() {
            ret |= 1 << 7;
        } else {
            ret &= !(1 << 7);
        }
        self.set_carry(val & 1 > 0);
        self.set_result_flags(ret);
        ret
    }

    // Branch opcodes

    // Utility function used by all branch instructions
    fn branch(&mut self, val: Byte) {
        let old_pc = self.pc;
        self.pc = self.pc.wrapping_add((val as i8) as Addr);
        self.cycles += 1;
        if Cpu::pages_differ(self.pc, old_pc) {
            self.cycles += 1;
        }
    }

    // BCC: Branch on Carry Clear
    fn bcc(&mut self, val: Byte) {
        if !self.carry() {
            self.branch(val);
        }
    }

    // BCS: Branch on Carry Set
    fn bcs(&mut self, val: Byte) {
        if self.carry() {
            self.branch(val);
        }
    }

    // BEQ: Branch on Result Zero
    fn beq(&mut self, val: Byte) {
        if self.zero() {
            self.branch(val);
        }
    }

    // BMI: Branch on Result Negative
    fn bmi(&mut self, val: Byte) {
        if self.negative() {
            self.branch(val);
        }
    }

    // BNE: Branch on Result Not Zero
    fn bne(&mut self, val: Byte) {
        if !self.zero() {
            self.branch(val);
        }
    }

    // BPL: Branch on Result Positive
    fn bpl(&mut self, val: Byte) {
        if !self.negative() {
            self.branch(val);
        }
    }

    // BVC: Branch on Overflow Clear
    fn bvc(&mut self, val: Byte) {
        if !self.overflow() {
            self.branch(val);
        }
    }

    // BVS: Branch on Overflow Set
    fn bvs(&mut self, val: Byte) {
        if self.overflow() {
            self.branch(val);
        }
    }

    // Jump opcodes

    // JMP: Jump to Location
    fn jmp(&mut self, addr: Addr) {
        self.pc = addr;
    }

    // JSR: Jump to Location Save Return addr
    fn jsr(&mut self, addr: Addr) {
        self.push_stackw(self.pc.wrapping_sub(1));
        self.pc = addr;
    }

    // RTI: Return from Interrupt
    fn rti(&mut self) {
        self.status = (self.pop_stackb() | UNUSED_FLAG) & !BREAK_FLAG;
        self.pc = self.pop_stackw();
    }

    // RTS: Return from Subroutine
    fn rts(&mut self) {
        self.pc = self.pop_stackw().wrapping_add(1);
    }

    // Register opcodes

    // CLC: Clear Carry Flag
    fn clc(&mut self) {
        self.set_carry(false);
    }

    // SEC: Set Carry Flag
    fn sec(&mut self) {
        self.set_carry(true);
    }

    // CLD: Clear Decimal Mode
    fn cld(&mut self) {
        self.set_decimal(false);
    }

    // SED: Set Decimal Mode
    fn sed(&mut self) {
        self.set_decimal(true);
    }

    // CLI: Clear Interrupt Disable Bit
    fn cli(&mut self) {
        self.set_interrupt_disable(false);
    }

    // SEI: Set Interrupt Disable Status
    fn sei(&mut self) {
        self.set_interrupt_disable(true);
    }

    // CLV: Clear Overflow Flag
    fn clv(&mut self) {
        self.set_overflow(false);
    }

    // Compare opcodes

    // Utility function used by all compare instructions
    fn compare(&mut self, a: Byte, b: Byte) {
        let result = a.wrapping_sub(b);
        self.set_result_flags(result);
        self.set_carry(a >= b);
    }

    // CMP: Compare M and A
    fn cmp(&mut self, val: Byte) {
        let a = self.acc;
        self.compare(a, val);
    }

    // CPX: Compare M and X
    fn cpx(&mut self, val: Byte) {
        let x = self.x;
        self.compare(x, val);
    }

    // CPY: Compare M and Y
    fn cpy(&mut self, val: Byte) {
        let y = self.y;
        self.compare(y, val);
    }

    // Stack opcodes

    // PHP: Push Processor Status on Stack
    fn php(&mut self) {
        self.push_stackb(self.status | UNUSED_FLAG | BREAK_FLAG);
    }

    // PLP: Pull Processor Status from Stack
    fn plp(&mut self) {
        self.status = (self.pop_stackb() | UNUSED_FLAG) & !BREAK_FLAG;
    }

    // PHA: Push A on Stack
    fn pha(&mut self) {
        self.push_stackb(self.acc);
    }

    // PLA: Pull A from Stack
    fn pla(&mut self) {
        self.acc = self.pop_stackb();
        self.update_acc();
    }

    // System opcodes

    // BRK: Force Break Interrupt
    fn brk(&mut self) {
        self.push_stackw(self.pc);
        self.push_stackb(self.status | UNUSED_FLAG | BREAK_FLAG);
        self.php();
        self.sei();
        self.pc = self.mem.readw(IRQ_ADDR);
    }

    // NOP: No Operation
    fn nop(&mut self) {}

    // Unofficial opcodes

    // LAX: Shortcut for LDA then TAX
    fn lax(&mut self, val: Byte) {
        self.acc = val;
        self.x = val;
        self.update_acc();
    }

    // SAX: AND A with X
    fn sax(&mut self) -> Byte {
        self.acc & self.x
    }

    // DCP: Shortcut for DEC then CMP
    fn dcp(&mut self, val: Byte) -> Byte {
        let val = val.wrapping_sub(1);
        self.compare(self.acc, val);
        val
    }

    // ISC/ISB: Shortcut for INC then SBC
    fn isb(&mut self, val: Byte) -> Byte {
        let x = self.inc(val);
        self.sbc(x);
        x
    }

    // RLA: Shortcut for ROL then AND
    fn rla(&mut self, val: Byte) -> Byte {
        let x = self.rol(val);
        self.and(x);
        x
    }

    // RRA: Shortcut for ROR then ADC
    fn rra(&mut self, val: Byte) -> Byte {
        let x = self.ror(val);
        self.adc(x);
        x
    }

    // SLO: Shortcut for ASL then ORA
    fn slo(&mut self, val: Byte) -> Byte {
        let x = self.asl(val);
        self.ora(x);
        x
    }

    // SRA: Shortcut for LSR then EOR
    fn sre(&mut self, val: Byte) -> Byte {
        let x = self.lsr(val);
        self.eor(x);
        x
    }
}

// Since we're working with u8s, we need a way to check for negative numbers
fn is_negative(val: Byte) -> bool {
    val >= 128
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_new() {
        let mut cpu_memory = CpuMemMap::init();
        let c = Cpu::init(cpu_memory);
        assert_eq!(c.cycles, 0);
        assert_eq!(c.stall, 0);
        assert_eq!(c.pc, 0);
        assert_eq!(c.sp, 0);
        assert_eq!(c.acc, 0);
        assert_eq!(c.x, 0);
        assert_eq!(c.y, 0);
        assert_eq!(c.status, 0);
    }

    #[test]
    fn test_cpu_reset() {
        let mut cpu_memory = CpuMemMap::init();
        let mut c = Cpu::init(cpu_memory);
        c.reset();
        assert_eq!(c.pc, 0);
        assert_eq!(c.sp, RESET_SP);
        assert_eq!(c.status, RESET_STATUS);
        assert_eq!(c.cycles, 7);
    }
}
