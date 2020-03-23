//! A 6502 Central Processing Unit
//!
//! [http://wiki.nesdev.com/w/index.php/CPU]()

use crate::{
    bus::Bus,
    common::{Clocked, Powered},
    logging::{LogLevel, Loggable},
    memory::{MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use instr::{AddrMode::*, Instr, Operation::*, INSTRUCTIONS};
use std::{
    collections::VecDeque,
    fmt,
    io::{Read, Write},
};

pub mod instr;

// TODO 1.79 MHz (~559 ns/cycle) - May want to use 1_786_830 for a stable 60 FPS
// Add Emulator setting like Mesen??
// http://forums.nesdev.com/viewtopic.php?p=223679#p223679
// pub const MASTER_CLOCK_RATE: f32 = 21_441_960.0; // 21.441960 MHz Emulated clock rate

pub const MASTER_CLOCK_RATE: f32 = 21_477_270.0; // 21.47727 MHz Hardware clock rate
pub const CPU_CLOCK_RATE: f32 = MASTER_CLOCK_RATE / 12.0;

const NMI_ADDR: u16 = 0xFFFA; // NMI Vector address
const IRQ_ADDR: u16 = 0xFFFE; // IRQ Vector address
const RESET_ADDR: u16 = 0xFFFC; // Vector address at reset
const POWER_ON_SP: u8 = 0xFD; // Because reasons. Possibly because of NMI/IRQ/BRK messing with SP on reset
const POWER_ON_STATUS: u8 = 0x24; // 0010 0100 - Unused and Interrupt Disable set
const SP_BASE: u16 = 0x0100; // Stack-pointer starting address
const PC_LOG_LEN: usize = 20;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Irq {
    Reset = 1,
    Mapper = (1 << 1),
    FrameCounter = (1 << 2),
    Dmc = (1 << 3),
}

// Status Registers
// http://wiki.nesdev.com/w/index.php/Status_flags
// 7654 3210
// NVUB DIZC
// |||| ||||
// |||| |||+- Carry
// |||| ||+-- Zero
// |||| |+--- Interrupt Disable
// |||| +---- Decimal Mode - Not used in the NES but still has to function
// |||+------ Break - 1 when pushed to stack from PHP/BRK, 0 from IRQ/NMI
// ||+------- Unused - always set to 1 when pushed to stack
// |+-------- Overflow
// +--------- Negative
pub enum StatusRegs {
    C = 1,        // Carry
    Z = (1 << 1), // Zero
    I = (1 << 2), // Disable Interrupt
    D = (1 << 3), // Decimal Mode
    B = (1 << 4), // Break
    U = (1 << 5), // Unused
    V = (1 << 6), // Overflow
    N = (1 << 7), // Negative
}
use StatusRegs::*;

/// The Central Processing Unit status and registers
#[derive(Clone)]
pub struct Cpu {
    pub cycle_count: usize, // total number of cycles ran
    pub step: usize,        // total number of CPU instructions run
    pub pc: u16,            // program counter
    pub sp: u8,             // stack pointer - stack is at $0100-$01FF
    pub acc: u8,            // accumulator
    pub x: u8,              // x register
    pub y: u8,              // y register
    pub status: u8,         // Status Registers
    pub bus: Bus,
    pub pc_log: VecDeque<u16>,
    pub stall: usize,     // Number of cycles to stall with nop (used by DMA)
    pub instr: Instr,     // The currently executing instruction
    pub abs_addr: u16,    // Used memory addresses get set here
    pub rel_addr: u16,    // Relative address for branch instructions
    pub fetched_data: u8, // Represents data fetched for the ALU
    pub irq_pending: u8,  // Pending interrupts
    pub nmi_pending: bool,
    last_irq: bool,
    last_nmi: bool,
    log_level: LogLevel,
}

impl Cpu {
    pub fn init(bus: Bus) -> Self {
        Self {
            cycle_count: 0,
            step: 0,
            pc: 0x0000,
            sp: POWER_ON_SP,
            acc: 0x00,
            x: 0x00,
            y: 0x00,
            status: POWER_ON_STATUS,
            bus,
            pc_log: VecDeque::with_capacity(PC_LOG_LEN),
            stall: 0,
            instr: INSTRUCTIONS[0x00],
            abs_addr: 0x0000,
            rel_addr: 0x0000,
            fetched_data: 0x00,
            irq_pending: Irq::Reset as u8,
            nmi_pending: false,
            last_irq: false,
            last_nmi: false,
            log_level: LogLevel::default(),
        }
    }

    pub fn power_on(&mut self) {
        let pcl = u16::from(self.bus.read(RESET_ADDR));
        let pch = u16::from(self.bus.read(RESET_ADDR + 1));
        self.pc = (pch << 8) | pcl;
        self.set_irq(Irq::Reset, true);
    }

    pub fn next_instr(&self) -> Instr {
        let opcode = self.peek(self.pc);
        INSTRUCTIONS[opcode as usize]
    }

    /// Sends an IRQ Interrupt to the CPU
    ///
    /// http://wiki.nesdev.com/w/index.php/IRQ
    pub fn set_irq(&mut self, irq: Irq, val: bool) {
        if val {
            self.irq_pending |= irq as u8;
        } else {
            self.irq_pending &= !(irq as u8);
        }
    }

    /// Checks if a a given IRQ is active
    pub fn has_irq(&mut self, irq: Irq) -> bool {
        (self.irq_pending & irq as u8) > 0
    }

    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch PCH
    //  2    PC     R  fetch PCL
    //  3  $0100,S  W  push PCH to stack, decrement S
    //  4  $0100,S  W  push PCL to stack, decrement S
    //  5  $0100,S  W  push P to stack, decrement S
    //  6    PC     R  fetch low byte of interrupt vector
    //  7    PC     R  fetch high byte of interrupt vector
    pub fn irq(&mut self) {
        self.read(self.pc);
        self.read(self.pc);
        self.push_stackw(self.pc);
        // Set U and !B during push
        self.push_stackb((self.status | U as u8) & !(B as u8));
        self.set_flag(I, true);
        if self.has_irq(Irq::Reset) {
            self.pc = self.readw(RESET_ADDR);
            self.set_irq(Irq::Reset, false);
        } else if self.last_nmi {
            self.nmi_pending = false;
            self.bus.ppu.nmi_pending = false;
            self.pc = self.readw(NMI_ADDR);
        } else {
            self.pc = self.readw(IRQ_ADDR);
        }
        // Prevent NMI from triggering immediately after IRQ
        if self.last_nmi {
            self.last_nmi = false;
        }
    }

    /// Sends a NMI Interrupt to the CPU
    ///
    /// http://wiki.nesdev.com/w/index.php/NMI
    pub fn set_nmi(&mut self, val: bool) {
        self.nmi_pending = val;
        self.bus.ppu.nmi_pending = val;
    }

    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch PCH
    //  2    PC     R  fetch PCL
    //  3  $0100,S  W  push PCH to stack, decrement S
    //  4  $0100,S  W  push PCL to stack, decrement S
    //  5  $0100,S  W  push P to stack, decrement S
    //  6    PC     R  fetch low byte of interrupt vector
    //  7    PC     R  fetch high byte of interrupt vector
    fn nmi(&mut self) {
        self.read(self.pc);
        self.read(self.pc);
        self.push_stackw(self.pc);
        // Set U and !B during push
        self.push_stackb((self.status | U as u8) & !(B as u8));
        self.set_flag(I, true);
        self.pc = self.readw(NMI_ADDR);
    }

    fn run_cycle(&mut self) {
        self.cycle_count = self.cycle_count.wrapping_add(1);
        self.last_nmi = self.nmi_pending;
        self.last_irq = self.irq_pending > 0 && self.get_flag(I) == 0;
        let ppu_cycles = self.bus.ppu.clock();
        self.set_nmi(self.bus.ppu.nmi_pending);
        for _ in 0..ppu_cycles {
            let irq_pending = {
                let mut mapper = self.bus.mapper.borrow_mut();
                let _ = mapper.clock(); // Don't care how many cycles are run
                mapper.irq_pending()
            };
            self.set_irq(Irq::Mapper, irq_pending);
        }
        let _ = self.bus.apu.clock(); // Don't care how many cycles are run
        self.set_irq(Irq::FrameCounter, self.bus.apu.irq_pending);
        self.set_irq(Irq::Dmc, self.bus.apu.dmc.irq_pending);
    }

    // Status Register functions

    // Convenience method to set both Z and N
    fn set_flags_zn(&mut self, val: u8) {
        self.set_flag(Z, val == 0x00);
        self.set_flag(N, val & 0x80 == 0x80);
    }

    fn get_flag(&self, flag: StatusRegs) -> u8 {
        if (self.status & flag as u8) > 0 {
            1
        } else {
            0
        }
    }

    fn set_flag(&mut self, flag: StatusRegs, val: bool) {
        if val {
            self.status |= flag as u8;
        } else {
            self.status &= !(flag as u8);
        }
    }

    // Stack Functions

    // Push a byte to the stack
    fn push_stackb(&mut self, val: u8) {
        self.write(SP_BASE | u16::from(self.sp), val);
        self.sp = self.sp.wrapping_sub(1);
    }

    // Pull a byte from the stack
    fn pop_stackb(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.read(SP_BASE | u16::from(self.sp))
    }

    // Peek byte at the top of the stack
    pub fn peek_stackb(&self) -> u8 {
        let sp = self.sp.wrapping_add(1);
        self.peek(SP_BASE | u16::from(sp))
    }

    // Push a word (two bytes) to the stack
    fn push_stackw(&mut self, val: u16) {
        let lo = (val & 0xFF) as u8;
        let hi = (val >> 8) as u8;
        self.push_stackb(hi);
        self.push_stackb(lo);
    }

    // Pull a word (two bytes) from the stack
    fn pop_stackw(&mut self) -> u16 {
        let lo = u16::from(self.pop_stackb());
        let hi = u16::from(self.pop_stackb());
        hi << 8 | lo
    }

    // Peek at the top of the stack
    pub fn peek_stackw(&self) -> u16 {
        let sp = self.sp.wrapping_add(1);
        let lo = u16::from(self.peek(SP_BASE | u16::from(sp)));
        let sp = sp.wrapping_add(1);
        let hi = u16::from(self.peek(SP_BASE | u16::from(sp)));
        hi << 8 | lo
    }

    /// Addressing Modes
    ///
    /// The 6502 can address 64KB from 0x0000 - 0xFFFF. The high byte is usually the page and the
    /// low byte the offset into the page. There are 256 total pages of 256 bytes.

    // FIXME
    // 9E SXA #aby 6 > 5 (cross)
    // 9F AHX #aby 6 > 5 (cross)
    // A3 LAX #idx 5 > 6
    // B7 LAX #zpy 3 > 4
    // BB LAS #aby 5 > 4
    // BF LAX #aby 4 > 5 (cross)
    // C3 DCP #idx 7 > 8
    // D3 DCP #idy 7 > 8
    // D7 DCP #zpx 5 > 6
    // DB CLD #imp 6 > 7
    // DF DCP #abx 6 > 7
    // E3 ISB #idx 7 > 8
    // F3 ISB #idy 7 > 8
    // F7 ISB #zpx 5 > 6
    // FB ISB #aby 6 > 7
    // FF ISB #abx 6 > 7

    /// Accumulator
    /// No additional data is required, but the default target will be the accumulator.
    //  ASL, ROL, LSR, ROR
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    fn acc(&mut self) {
        let _ = self.read(self.pc); // Cycle 2, Read and throw away
    }

    /// Implied
    /// No additional data is required, but the default target will be the accumulator.
    // #  address R/W description
    //   --- ------- --- -----------------------------------------------
    //    1    PC     R  fetch opcode, increment PC
    //    2    PC     R  read next instruction byte (and throw it away)
    fn imp(&mut self) {
        let _ = self.read(self.pc); // Cycle 2, Read and throw away
    }

    /// Immediate
    /// Uses the next byte as the value, so we'll update the abs_addr to the next byte.
    // #  address R/W description
    //   --- ------- --- ------------------------------------------
    //    1    PC     R  fetch opcode, increment PC
    //    2    PC     R  fetch value, increment PC
    fn imm(&mut self) {
        self.abs_addr = self.pc;
        self.pc = self.pc.wrapping_add(1);
    }

    /// Zero Page
    /// Accesses the first 0xFF bytes of the address range, so this only requires one extra byte
    /// instead of the usual two.
    //  Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                    LAX, NOP)

    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch address, increment PC
    //     3  address  R  read from effective address

    //  Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                  SLO, SRE, RLA, RRA, ISB, DCP)

    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch address, increment PC
    //     3  address  R  read from effective address
    //     4  address  W  write the value back to effective address,
    //                    and do the operation on it
    //     5  address  W  write the new value to effective address

    //  Write instructions (STA, STX, STY, SAX)

    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch address, increment PC
    //     3  address  W  write register to effective address
    fn zp0(&mut self) {
        self.abs_addr = u16::from(self.read(self.pc)) & 0x00FF; // Cycle 2
        self.pc = self.pc.wrapping_add(1);
    }

    /// Zero Page w/ X offset
    /// Same as Zero Page, but is offset by adding the x register.
    //  Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                     LAX, NOP)

    //     #   address  R/W description
    //    --- --------- --- ------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register to it
    //     4  address+X* R  read from effective address

    //           * The high byte of the effective address is always zero,
    //             i.e. page boundary crossings are not handled.

    //  Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                  SLO, SRE, RLA, RRA, ISB, DCP)

    //     #   address  R/W description
    //    --- --------- --- ---------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register X to it
    //     4  address+X* R  read from effective address
    //     5  address+X* W  write the value back to effective address,
    //                      and do the operation on it
    //     6  address+X* W  write the new value to effective address

    //    Note: * The high byte of the effective address is always zero,
    //            i.e. page boundary crossings are not handled.

    //  Write instructions (STA, STX, STY, SAX)

    //     #   address  R/W description
    //    --- --------- --- -------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register to it
    //     4  address+X* W  write to effective address

    //           * The high byte of the effective address is always zero,
    //             i.e. page boundary crossings are not handled.
    fn zpx(&mut self) {
        let addr = self.read(self.pc); // Cycle 2
        self.abs_addr = u16::from(addr.wrapping_add(self.x)) & 0x00FF;
        let _ = self.read(u16::from(addr)); // Cycle 3
        self.pc = self.pc.wrapping_add(1);
    }

    /// Zero Page w/ Y offset
    /// Same as Zero Page, but is offset by adding the y register.
    //  Read instructions (LDX, LAX)

    //     #   address  R/W description
    //    --- --------- --- ------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register to it
    //     4  address+Y* R  read from effective address

    //           * The high byte of the effective address is always zero,
    //             i.e. page boundary crossings are not handled.

    //  Write instructions (STX, SAX)

    //     #   address  R/W description
    //    --- --------- --- -------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register to it
    //     4  address+Y* W  write to effective address

    //           * The high byte of the effective address is always zero,
    //             i.e. page boundary crossings are not handled.
    fn zpy(&mut self) {
        let addr = self.read(self.pc); // Cycle 2
        self.abs_addr = u16::from(addr.wrapping_add(self.y)) & 0x00FF;
        let _ = self.read(u16::from(addr)); // Cycle 3
        self.pc = self.pc.wrapping_add(1);
    }

    /// Relative
    /// This mode is only used by branching instructions. The address must be between -128 and +127,
    /// allowing the branching instruction to move backward or forward relative to the current
    /// program counter.
    //    #   address  R/W description
    //   --- --------- --- ---------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch operand, increment PC
    //    3     PC      R  Fetch opcode of next instruction,
    //                     If branch is taken, add operand to PCL.
    //                     Otherwise increment PC.
    //    4+    PC*     R  Fetch opcode of next instruction.
    //                     Fix PCH. If it did not change, increment PC.
    //    5!    PC      R  Fetch opcode of next instruction,
    //                     increment PC.

    //   Notes: The opcode fetch of the next instruction is included to
    //          this diagram for illustration purposes. When determining
    //          real execution times, remember to subtract the last
    //          cycle.

    //          * The high byte of Program Counter (PCH) may be invalid
    //            at this time, i.e. it may be smaller or bigger by $100.

    //          + If branch is taken, this cycle will be executed.

    //          ! If branch occurs to different page, this cycle will be
    //            executed.
    fn rel(&mut self) {
        self.rel_addr = self.read(self.pc).into(); // Cycle 2
        self.pc = self.pc.wrapping_add(1);
    }

    /// Absolute
    /// Uses a full 16-bit address as the next value.
    //  Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                     LAX, NOP)
    //
    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch low byte of address, increment PC
    //     3    PC     R  fetch high byte of address, increment PC
    //     4  address  R  read from effective address

    //  Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                  SLO, SRE, RLA, RRA, ISB, DCP)
    //
    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch low byte of address, increment PC
    //     3    PC     R  fetch high byte of address, increment PC
    //     4  address  R  read from effective address
    //     5  address  W  write the value back to effective address,
    //                    and do the operation on it
    //     6  address  W  write the new value to effective address

    //  Write instructions (STA, STX, STY, SAX)
    //
    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch low byte of address, increment PC
    //     3    PC     R  fetch high byte of address, increment PC
    //     4  address  W  write register to effective address
    fn abs(&mut self) {
        self.abs_addr = self.readw(self.pc); // Cycle 2 & 3
        self.pc = self.pc.wrapping_add(2);
    }

    /// Absolute w/ X offset
    /// Same as Absolute, but is offset by adding the x register. If a page boundary is crossed, an
    /// additional clock is required.
    // Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                    LAX, LAE, SHS, NOP)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch low byte of address, increment PC
    //    3     PC      R  fetch high byte of address,
    //                     add index register to low address byte,
    //                     increment PC
    //    4  address+X* R  read from effective address,
    //                     fix the high byte of effective address
    //    5+ address+X  R  re-read from effective address

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    //          + This cycle will be executed only if the effective address
    //            was invalid during cycle #4, i.e. page boundary was crossed.

    // Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                 SLO, SRE, RLA, RRA, ISB, DCP)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1    PC       R  fetch opcode, increment PC
    //    2    PC       R  fetch low byte of address, increment PC
    //    3    PC       R  fetch high byte of address,
    //                     add index register X to low address byte,
    //                     increment PC
    //    4  address+X* R  read from effective address,
    //                     fix the high byte of effective address
    //    5  address+X  R  re-read from effective address
    //    6  address+X  W  write the value back to effective address,
    //                     and do the operation on it
    //    7  address+X  W  write the new value to effective address

    //   Notes: * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    // Write instructions (STA, STX, STY, SHA, SHX, SHY)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch low byte of address, increment PC
    //    3     PC      R  fetch high byte of address,
    //                     add index register to low address byte,
    //                     increment PC
    //    4  address+X* R  read from effective address,
    //                     fix the high byte of effective address
    //    5  address+X  W  write to effective address

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100. Because
    //            the processor cannot undo a write to an invalid
    //            address, it always reads from the address first.
    fn abx(&mut self) {
        let addr = self.readw(self.pc); // Cycle 2 & 3
        self.pc = self.pc.wrapping_add(2);
        self.abs_addr = addr.wrapping_add(self.x.into());
        // Cycle 4 Read with fixed high byte
        self.fetched_data = self.read((addr & 0xFF00) | (self.abs_addr & 0x00FF));
    }

    /// Absolute w/ Y offset
    /// Same as Absolute, but is offset by adding the y register. If a page boundary is crossed, an
    /// additional clock is required.
    // Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                    LAX, LAE, SHS, NOP)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch low byte of address, increment PC
    //    3     PC      R  fetch high byte of address,
    //                     add index register to low address byte,
    //                     increment PC
    //    4  address+Y* R  read from effective address,
    //                     fix the high byte of effective address
    //    5+ address+Y  R  re-read from effective address

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    //          + This cycle will be executed only if the effective address
    //            was invalid during cycle #4, i.e. page boundary was crossed.

    // Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                 SLO, SRE, RLA, RRA, ISB, DCP)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1    PC       R  fetch opcode, increment PC
    //    2    PC       R  fetch low byte of address, increment PC
    //    3    PC       R  fetch high byte of address,
    //                     add index register Y to low address byte,
    //                     increment PC
    //    4  address+Y* R  read from effective address,
    //                     fix the high byte of effective address
    //    5  address+Y  R  re-read from effective address
    //    6  address+Y  W  write the value back to effective address,
    //                     and do the operation on it
    //    7  address+Y  W  write the new value to effective address

    //   Notes: * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    // Write instructions (STA, STX, STY, SHA, SHX, SHY)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch low byte of address, increment PC
    //    3     PC      R  fetch high byte of address,
    //                     add index register to low address byte,
    //                     increment PC
    //    4  address+Y* R  read from effective address,
    //                     fix the high byte of effective address
    //    5  address+Y  W  write to effective address

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100. Because
    //            the processor cannot undo a write to an invalid
    //            address, it always reads from the address first.
    fn aby(&mut self) {
        let addr = self.readw(self.pc); // Cycles 2 & 3
        self.pc = self.pc.wrapping_add(2);
        self.abs_addr = addr.wrapping_add(self.y.into());
        // Cycle 4 Read with fixed high byte
        self.fetched_data = self.read((addr & 0xFF00) | (self.abs_addr & 0x00FF));
    }

    /// Indirect (JMP)
    /// The next 16-bit address is used to get the actual 16-bit address. This instruction has
    /// a bug in the original hardware. If the lo byte is 0xFF, the hi byte would cross a page
    /// boundary. However, this doesn't work correctly on the original hardware and instead
    /// wraps back around to 0.
    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch pointer address low, increment PC
    //    3     PC      R  fetch pointer address high, increment PC
    //    4   pointer   R  fetch low address to latch
    //    5  pointer+1* R  fetch PCH, copy latch to PCL

    //   Note: * The PCH will always be fetched from the same page
    //           than PCL, i.e. page boundary crossing is not handled.

    //            How Real Programmers Acknowledge Interrupts
    fn ind(&mut self) {
        let addr = self.readw(self.pc);
        self.pc = self.pc.wrapping_add(2);
        if addr & 0x00FF == 0x00FF {
            // Simulate bug
            self.abs_addr = (u16::from(self.read(addr & 0xFF00)) << 8) | u16::from(self.read(addr));
        } else {
            // Normal behavior
            self.abs_addr = (u16::from(self.read(addr + 1)) << 8) | u16::from(self.read(addr));
        }
    }

    /// Indirect X
    /// The next 8-bit address is offset by the X register to get the actual 16-bit address from
    /// page 0x00.
    // Read instructions (LDA, ORA, EOR, AND, ADC, CMP, SBC, LAX)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  read from the address, add X to it
    //    4   pointer+X   R  fetch effective address low
    //    5  pointer+X+1  R  fetch effective address high
    //    6    address    R  read from effective address

    //   Note: The effective address is always fetched from zero page,
    //         i.e. the zero page boundary crossing is not handled.

    // Read-Modify-Write instructions (SLO, SRE, RLA, RRA, ISB, DCP)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  read from the address, add X to it
    //    4   pointer+X   R  fetch effective address low
    //    5  pointer+X+1  R  fetch effective address high
    //    6    address    R  read from effective address
    //    7    address    W  write the value back to effective address,
    //                       and do the operation on it
    //    8    address    W  write the new value to effective address

    //   Note: The effective address is always fetched from zero page,
    //         i.e. the zero page boundary crossing is not handled.

    // Write instructions (STA, SAX)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  read from the address, add X to it
    //    4   pointer+X   R  fetch effective address low
    //    5  pointer+X+1  R  fetch effective address high
    //    6    address    W  write to effective address

    //   Note: The effective address is always fetched from zero page,
    //         i.e. the zero page boundary crossing is not handled.
    fn idx(&mut self) {
        let addr = self.read(self.pc); // Cycle 2
        self.pc = self.pc.wrapping_add(1);
        let x_offset = addr.wrapping_add(self.x);
        let _ = self.read(u16::from(addr)); // Cycle 3
        self.abs_addr = self.readw_zp(x_offset); // Cycles 4 & 5
    }

    /// Indirect Y
    /// The next 8-bit address is read to get a 16-bit address from page 0x00, which is then offset
    /// by the Y register. If a page boundary is crossed, add a clock cycle.
    // Read instructions (LDA, EOR, AND, ORA, ADC, SBC, CMP)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  fetch effective address low
    //    4   pointer+1   R  fetch effective address high,
    //                       add Y to low byte of effective address
    //    5   address+Y*  R  read from effective address,
    //                       fix high byte of effective address
    //    6+  address+Y   R  read from effective address

    //   Notes: The effective address is always fetched from zero page,
    //          i.e. the zero page boundary crossing is not handled.

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    //          + This cycle will be executed only if the effective address
    //            was invalid during cycle #5, i.e. page boundary was crossed.

    // Read-Modify-Write instructions (SLO, SRE, RLA, RRA, ISB, DCP)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  fetch effective address low
    //    4   pointer+1   R  fetch effective address high,
    //                       add Y to low byte of effective address
    //    5   address+Y*  R  read from effective address,
    //                       fix high byte of effective address
    //    6   address+Y   R  re-read from effective address
    //    7   address+Y   W  write the value back to effective address,
    //                       and do the operation on it
    //    8   address+Y   W  write the new value to effective address

    //   Notes: The effective address is always fetched from zero page,
    //          i.e. the zero page boundary crossing is not handled.

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    // Write instructions (STA, SHA)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  fetch effective address low
    //    4   pointer+1   R  fetch effective address high,
    //                       add Y to low byte of effective address
    //    5   address+Y*  R  read from effective address,
    //                       fix high byte of effective address
    //    6   address+Y   W  write to effective address

    //   Notes: The effective address is always fetched from zero page,
    //          i.e. the zero page boundary crossing is not handled.

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.
    fn idy(&mut self) {
        let addr = self.read(self.pc); // Cycle 2
        self.pc = self.pc.wrapping_add(1);
        let addr = self.readw_zp(addr); // Cycles 3 & 4
        self.abs_addr = addr.wrapping_add(self.y.into());
        // Cycle 4 Read with fixed high byte
        self.fetched_data = self.read((addr & 0xFF00) | (self.abs_addr & 0x00FF));
    }

    // Source the data used by an instruction. Some instructions don't fetch data as the source
    // is implied by the instruction such as INX which increments the X register.
    fn fetch_data(&mut self) {
        let mode = self.instr.addr_mode();
        self.fetched_data = match mode {
            IMP | ACC => self.acc,
            ABX | ABY | IDY => {
                // Read instructions may have crossed a page boundary and need to be re-read
                match self.instr.op() {
                    LDA | LDX | LDY | EOR | AND | ORA | ADC | SBC | CMP | BIT | LAX | NOP | IGN
                    | LAS => {
                        let reg = match mode {
                            ABX => self.x,
                            ABY | IDY => self.y,
                            _ => panic!("not possible"),
                        };
                        // Read if we crossed, otherwise use what was already read
                        if (self.abs_addr & 0x00FF) < u16::from(reg) {
                            self.read(self.abs_addr)
                        } else {
                            self.fetched_data
                        }
                    }
                    _ => self.read(self.abs_addr), // Cycle 2/4/5 read
                }
            }
            _ => self.read(self.abs_addr), // Cycle 2/4/5 read
        };
    }

    // Writes data back to where fetched_data was sourced from. Either accumulator or memory
    // specified in abs_addr.
    fn write_fetched(&mut self, val: u8) {
        match self.instr.addr_mode() {
            IMP | ACC => self.acc = val,
            IMM => (), // noop
            _ => self.write(self.abs_addr, val),
        }
    }

    // Memory accesses

    // Utility to read a full 16-bit word
    pub fn readw(&mut self, addr: u16) -> u16 {
        let lo = u16::from(self.read(addr));
        let hi = u16::from(self.read(addr.wrapping_add(1)));
        (hi << 8) | lo
    }

    // readw but don't accidentally modify state
    pub fn peekw(&self, addr: u16) -> u16 {
        let lo = u16::from(self.peek(addr));
        let hi = u16::from(self.peek(addr.wrapping_add(1)));
        (hi << 8) | lo
    }

    // Like readw, but for Zero Page which means it'll wrap around at 0xFF
    fn readw_zp(&mut self, addr: u8) -> u16 {
        let lo = u16::from(self.read(addr.into()));
        let hi = u16::from(self.read(addr.wrapping_add(1).into()));
        (hi << 8) | lo
    }

    // Like peekw, but for Zero Page which means it'll wrap around at 0xFF
    fn peekw_zp(&self, addr: u8) -> u16 {
        let lo = u16::from(self.peek(addr.into()));
        let hi = u16::from(self.peek(addr.wrapping_add(1).into()));
        (hi << 8) | lo
    }

    // Copies data to the PPU OAMDATA ($2004) using DMA (Direct Memory Access)
    // http://wiki.nesdev.com/w/index.php/PPU_registers#OAMDMA
    fn write_oamdma(&mut self, addr: u8) {
        let mut addr = u16::from(addr) << 8; // Start at $XX00
        let oam_addr = 0x2004;
        self.run_cycle(); // Dummy cyle to wait for writes to complete
        if self.cycle_count & 0x01 == 1 {
            // +1 cycle if on an odd cycle
            self.run_cycle();
        }
        for _ in 0..256 {
            // Copy 256 bytes from $XX00-$XXFF
            let val = self.read(addr);
            self.write(oam_addr, val);
            addr = addr.saturating_add(1);
        }
    }

    pub fn disassemble(&self, pc: &mut u16) -> String {
        let opcode = self.peek(*pc);
        let instr = INSTRUCTIONS[opcode as usize];
        let mut bytes = Vec::new();
        let mut disasm = String::with_capacity(50);
        disasm.push_str(&format!("${:04X}:", pc));
        bytes.push(self.peek(*pc));
        *pc = pc.wrapping_add(1);
        let mode = match instr.addr_mode() {
            IMM => {
                bytes.push(self.peek(*pc));
                *pc = pc.wrapping_add(1);
                format!("#${:02X}", bytes[1])
            }
            ZP0 => {
                bytes.push(self.peek(*pc));
                *pc = pc.wrapping_add(1);
                let val = self.peek(bytes[1].into());
                format!("${:02X} = #${:02X}", bytes[1], val)
            }
            ZPX => {
                bytes.push(self.peek(*pc));
                *pc = pc.wrapping_add(1);
                let x_offset = bytes[1].wrapping_add(self.x);
                let val = self.peek(x_offset.into());
                format!("${:02X},X @ ${:04X} = #${:02X}", bytes[1], x_offset, val)
            }
            ZPY => {
                bytes.push(self.peek(*pc));
                *pc = pc.wrapping_add(1);
                let y_offset = bytes[1].wrapping_add(self.y);
                let val = self.peek(y_offset.into());
                format!("${:02X},Y @ ${:02X} = #${:02X}", bytes[1], y_offset, val)
            }
            ABS => {
                bytes.push(self.peek(*pc));
                bytes.push(self.peek(pc.wrapping_add(1)));
                let addr = self.peekw(*pc);
                *pc = pc.wrapping_add(2);
                if instr.op() == JMP || instr.op() == JSR {
                    format!("${:04X}", addr)
                } else {
                    let val = self.peek(addr);
                    format!("${:04X} = #${:02X}", addr, val)
                }
            }
            ABX => {
                bytes.push(self.peek(*pc));
                bytes.push(self.peek(pc.wrapping_add(1)));
                let addr = self.peekw(*pc);
                *pc = pc.wrapping_add(2);
                let x_offset = addr.wrapping_add(self.x.into());
                let val = self.peek(x_offset);
                format!("${:04X},X @ ${:04X} = #${:02X}", addr, x_offset, val)
            }
            ABY => {
                bytes.push(self.peek(*pc));
                bytes.push(self.peek(pc.wrapping_add(1)));
                let addr = self.peekw(*pc);
                *pc = pc.wrapping_add(2);
                let y_offset = addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset);
                format!("${:04X},Y @ ${:04X} = #${:02X}", addr, y_offset, val)
            }
            IND => {
                bytes.push(self.peek(*pc));
                bytes.push(self.peek(pc.wrapping_add(1)));
                let addr = self.peekw(*pc);
                *pc = pc.wrapping_add(2);
                let val = if addr & 0x00FF == 0x00FF {
                    (u16::from(self.peek(addr & 0xFF00)) << 8) | u16::from(self.peek(addr))
                } else {
                    (u16::from(self.peek(addr + 1)) << 8) | u16::from(self.peek(addr))
                };
                format!("(${:04X}) = ${:04X}", addr, val)
            }
            IDX => {
                bytes.push(self.peek(*pc));
                *pc = pc.wrapping_add(1);
                let x_offset = bytes[1].wrapping_add(self.x);
                let addr = self.peekw_zp(x_offset);
                let val = self.peek(addr);
                format!("(${:02X},X) @ ${:04X} = #${:02X}", bytes[1], addr, val)
            }
            IDY => {
                bytes.push(self.peek(*pc));
                *pc = pc.wrapping_add(1);
                let addr = self.peekw_zp(bytes[1]);
                let y_offset = addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset);
                format!("(${:02X}),Y @ ${:04X} = #${:02X}", bytes[1], y_offset, val)
            }
            REL => {
                bytes.push(self.peek(*pc));
                let mut rel_addr = self.peek(*pc).into();
                *pc = pc.wrapping_add(1);
                if rel_addr & 0x80 == 0x80 {
                    // If address is negative, extend sign to 16-bits
                    rel_addr |= 0xFF00;
                }
                format!("${:04X}", pc.wrapping_add(rel_addr))
            }
            ACC => "".to_string(),
            IMP => "".to_string(),
        };
        for i in 0..3 {
            if i < bytes.len() {
                disasm.push_str(&format!("{:02X} ", bytes[i]));
            } else {
                disasm.push_str(&"   ".to_string());
            }
        }
        disasm.push_str(&format!("{:?} {}", instr, mode));
        disasm
    }

    // Print the current instruction and status
    pub fn print_instruction(&mut self, mut pc: u16) {
        let disasm = self.disassemble(&mut pc);

        let status_flags = vec!['n', 'v', '-', 'b', 'd', 'i', 'z', 'c'];
        let mut status_str = String::with_capacity(8);
        for (i, s) in status_flags.iter().enumerate() {
            if ((self.status >> (7 - i)) & 1) > 0 {
                status_str.push(s.to_ascii_uppercase());
            } else {
                status_str.push(*s);
            }
        }
        println!(
            "{:<50} A:{:02X} X:{:02X} Y:{:02X} P:{} SP:{:02X} PPU:{:3},{:3} CYC:{}",
            disasm,
            self.acc,
            self.x,
            self.y,
            status_str,
            self.sp,
            self.bus.ppu.cycle,
            self.bus.ppu.scanline,
            self.cycle_count,
        );
    }

    /// Utilities

    fn pages_differ(&self, addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }
}

impl Clocked for Cpu {
    /// Runs the CPU one instruction
    fn clock(&mut self) -> usize {
        if self.stall > 0 {
            self.cycle_count = self.cycle_count.wrapping_add(1);
            self.stall -= 1;
            return 1;
        }

        let start_cycles = self.cycle_count;

        if self.has_irq(Irq::Reset) {
            self.irq();
        } else if self.last_nmi {
            self.nmi_pending = false;
            self.bus.ppu.nmi_pending = false;
            self.nmi();
        } else if self.last_irq {
            self.irq();
        }

        if self.log_level == LogLevel::Trace {
            self.print_instruction(self.pc);
        }
        self.pc_log.push_front(self.pc);
        if self.pc_log.len() > PC_LOG_LEN {
            self.pc_log.pop_back();
        }

        let opcode = self.read(self.pc); // Cycle 1 of instruction
        self.pc = self.pc.wrapping_add(1);
        self.instr = INSTRUCTIONS[opcode as usize];

        match self.instr.addr_mode() {
            IMM => self.imm(),
            ZP0 => self.zp0(),
            ZPX => self.zpx(),
            ZPY => self.zpy(),
            ABS => self.abs(),
            ABX => self.abx(),
            ABY => self.aby(),
            IND => self.ind(),
            IDX => self.idx(),
            IDY => self.idy(),
            REL => self.rel(),
            ACC => self.acc(),
            IMP => self.imp(),
        };

        match self.instr.op() {
            ADC => self.adc(), // ADd with Carry M with A
            AND => self.and(), // AND M with A
            ASL => self.asl(), // Arithmatic Shift Left M or A
            BCC => self.bcc(), // Branch on Carry Clear
            BCS => self.bcs(), // Branch if Carry Set
            BEQ => self.beq(), // Branch if EQual to zero
            BIT => self.bit(), // Test BITs of M with A (Affects N, V and Z)
            BMI => self.bmi(), // Branch on MInus (negative)
            BNE => self.bne(), // Branch if Not Equal to zero
            BPL => self.bpl(), // Branch on PLus (positive)
            BRK => self.brk(), // BReaK (forced interrupt)
            BVC => self.bvc(), // Branch if no oVerflow Set
            BVS => self.bvs(), // Branch on oVerflow Set
            CLC => self.clc(), // CLear Carry flag
            CLD => self.cld(), // CLear Decimal mode
            CLI => self.cli(), // CLear Interrupt disable
            CLV => self.clv(), // CLear oVerflow flag
            CMP => self.cmp(), // CoMPare
            CPX => self.cpx(), // ComPare with X
            CPY => self.cpy(), // ComPare with Y
            DEC => self.dec(), // DECrement M or A
            DEX => self.dex(), // DEcrement X
            DEY => self.dey(), // DEcrement Y
            EOR => self.eor(), // Exclusive-OR M with A
            INC => self.inc(), // INCrement M or A
            INX => self.inx(), // INcrement X
            INY => self.iny(), // INcrement Y
            JMP => self.jmp(), // JuMP - safe to unwrap because JMP is Absolute
            JSR => self.jsr(), // Jump and Save Return addr - safe to unwrap because JSR is Absolute
            LDA => self.lda(), // LoaD A with M
            LDX => self.ldx(), // LoaD X with M
            LDY => self.ldy(), // LoaD Y with M
            LSR => self.lsr(), // Logical Shift Right M or A
            NOP => self.nop(), // NO oPeration
            SKB => self.skb(), // Like NOP, but issues a dummy read
            IGN => self.ign(), // Like NOP, but issues a dummy read
            ORA => self.ora(), // OR with A
            PHA => self.pha(), // PusH A to the stack
            PHP => self.php(), // PusH Processor status to the stack
            PLA => self.pla(), // PulL A from the stack
            PLP => self.plp(), // PulL Processor status from the stack
            ROL => self.rol(), // ROtate Left M or A
            ROR => self.ror(), // ROtate Right M or A
            RTI => self.rti(), // ReTurn from Interrupt
            RTS => self.rts(), // ReTurn from Subroutine
            SBC => self.sbc(), // Subtract M from A with carry
            SEC => self.sec(), // SEt Carry flag
            SED => self.sed(), // SEt Decimal mode
            SEI => self.sei(), // SEt Interrupt disable
            STA => self.sta(), // STore A into M
            STX => self.stx(), // STore X into M
            STY => self.sty(), // STore Y into M
            TAX => self.tax(), // Transfer A to X
            TAY => self.tay(), // Transfer A to Y
            TSX => self.tsx(), // Transfer SP to X
            TXA => self.txa(), // TRansfer X to A
            TXS => self.txs(), // Transfer X to SP
            TYA => self.tya(), // Transfer Y to A
            ISB => self.isb(), // INC & SBC
            DCP => self.dcp(), // DEC & CMP
            AXS => self.axs(), // (A & X) - val into X
            LAS => self.las(), // LDA & TSX
            LAX => self.lax(), // LDA & TAX
            AHX => self.ahx(), // Store A & X & H in M
            SAX => self.sax(), // Sotre A & X in M
            XAA => self.xaa(), // TXA & AND
            SXA => self.sxa(), // Store X & H in M
            RRA => self.rra(), // ROR & ADC
            TAS => self.tas(), // STA & TXS
            SYA => self.sya(), // Store Y & H in M
            ARR => self.arr(), // AND #imm & ROR
            SRE => self.sre(), // LSR & EOR
            ALR => self.alr(), // AND #imm & LSR
            RLA => self.rla(), // ROL & AND
            ANC => self.anc(), // AND #imm
            SLO => self.slo(), // ASL & ORA
            XXX => self.xxx(), // Unimplemented opcode
        };

        self.step += 1;
        self.cycle_count - start_cycles
    }
}

impl MemRead for Cpu {
    fn read(&mut self, addr: u16) -> u8 {
        self.run_cycle();
        self.bus.read(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        self.bus.peek(addr)
    }
}
impl MemWrite for Cpu {
    fn write(&mut self, addr: u16, val: u8) {
        if addr == 0x4014 {
            self.write_oamdma(val);
        } else {
            self.run_cycle();
            self.bus.write(addr, val);
        }
    }
}

impl Powered for Cpu {
    /// Resets the CPU
    ///
    /// Updates the PC, SP, and Status values to defined constants.
    ///
    /// These operations take the CPU 7 cycle.
    fn reset(&mut self) {
        self.bus.reset();
        self.cycle_count = 0;
        self.stall = 0;
        self.sp = self.sp.saturating_sub(3);
        self.set_flag(I, true);
        self.pc_log.clear();
        self.power_on();
    }

    /// Power cycle the CPU
    ///
    /// Updates all status as if powered on for the first time
    ///
    /// These operations take the CPU 7 cycle.
    fn power_cycle(&mut self) {
        self.bus.power_cycle();
        self.cycle_count = 0;
        self.stall = 0;
        self.sp = POWER_ON_SP;
        self.acc = 0x00;
        self.x = 0x00;
        self.y = 0x00;
        self.status = POWER_ON_STATUS;
        self.pc_log.clear();
        self.power_on();
    }
}

impl Loggable for Cpu {
    fn set_log_level(&mut self, level: LogLevel) {
        self.log_level = level;
    }
    fn log_level(&self) -> LogLevel {
        self.log_level
    }
}

impl Savable for Cpu {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.cycle_count.save(fh)?;
        self.step.save(fh)?;
        self.pc.save(fh)?;
        self.sp.save(fh)?;
        self.acc.save(fh)?;
        self.x.save(fh)?;
        self.y.save(fh)?;
        self.status.save(fh)?;
        self.bus.save(fh)?;
        // Ignore pc_log
        self.stall.save(fh)?;
        self.instr.save(fh)?;
        self.abs_addr.save(fh)?;
        self.rel_addr.save(fh)?;
        self.fetched_data.save(fh)?;
        self.irq_pending.save(fh)?;
        self.nmi_pending.save(fh)?;
        self.last_irq.save(fh)?;
        self.last_nmi.save(fh)?;
        // Ignore log_level
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.cycle_count.load(fh)?;
        self.step.load(fh)?;
        self.pc.load(fh)?;
        self.sp.load(fh)?;
        self.acc.load(fh)?;
        self.x.load(fh)?;
        self.y.load(fh)?;
        self.status.load(fh)?;
        self.bus.load(fh)?;
        self.stall.load(fh)?;
        self.instr.load(fh)?;
        self.abs_addr.load(fh)?;
        self.rel_addr.load(fh)?;
        self.fetched_data.load(fh)?;
        self.irq_pending.load(fh)?;
        self.nmi_pending.load(fh)?;
        self.last_irq.load(fh)?;
        self.last_nmi.load(fh)?;
        Ok(())
    }
}

impl fmt::Debug for Cpu {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(
            f,
            "Cpu {{ {:04X} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{} rel_addr:{} }}",
            self.pc,
            self.acc,
            self.x,
            self.y,
            self.status,
            self.sp,
            self.cycle_count,
            self.rel_addr
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory;

    #[test]
    fn cpu_cycle_timing() {
        let mut cpu = Cpu::init(Bus::new());
        cpu.power_on();

        assert_eq!(cpu.cycle_count, 7, "cpu after power");
        assert_eq!(cpu.bus.ppu.cycle_count, 0, "ppu after power");

        // TODO test extra dummy read cases for ABX, ABY, REL, IDY
        // TODO add tests for branch page crossing

        for instr in INSTRUCTIONS.iter() {
            let extra_cycle = match instr.op() {
                BCC | BNE | BPL | BVC => 1,
                _ => 0,
            };
            // Ignore invalid opcodes
            if instr.op() == XXX {
                continue;
            }
            cpu.pc = 0;
            cpu.cycle_count = 0;
            cpu.bus.ppu.cycle_count = 0;
            cpu.status = POWER_ON_STATUS;
            cpu.acc = 0;
            cpu.x = 0;
            cpu.y = 0;
            cpu.bus.wram = Memory::ram_from_bytes(&[instr.opcode(), 0, 0, 0]);
            cpu.clock();
            let cpu_cyc = instr.cycles() + extra_cycle;
            let ppu_cyc = 3 * (instr.cycles() + extra_cycle);
            assert_eq!(
                cpu.cycle_count,
                cpu_cyc,
                "cpu ${:02X} {:?} #{:?}",
                instr.opcode(),
                instr.op(),
                instr.addr_mode()
            );
            assert_eq!(
                cpu.bus.ppu.cycle_count,
                ppu_cyc as usize,
                "ppu ${:02X} {:?} #{:?}",
                instr.opcode(),
                instr.op(),
                instr.addr_mode()
            );
        }
    }
}
