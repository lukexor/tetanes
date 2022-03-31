//! A 6502 Central Processing Unit
//!
//! <http://wiki.nesdev.com/w/index.php/CPU>

use crate::{
    bus::Bus,
    common::{Clocked, Powered},
    mapper::Mapped,
    memory::{MemRead, MemWrite},
};
use bitflags::bitflags;
use instr::{
    AddrMode::{ABS, ABX, ABY, ACC, IDX, IDY, IMM, IMP, IND, REL, ZP0, ZPX, ZPY},
    Instr,
    Operation::{
        ADC, AHX, ALR, ANC, AND, ARR, ASL, AXS, BCC, BCS, BEQ, BIT, BMI, BNE, BPL, BRK, BVC, BVS,
        CLC, CLD, CLI, CLV, CMP, CPX, CPY, DCP, DEC, DEX, DEY, EOR, IGN, INC, INX, INY, ISB, JMP,
        JSR, LAS, LAX, LDA, LDX, LDY, LSR, NOP, ORA, PHA, PHP, PLA, PLP, RLA, ROL, ROR, RRA, RTI,
        RTS, SAX, SBC, SEC, SED, SEI, SKB, SLO, SRE, STA, STX, STY, SXA, SYA, TAS, TAX, TAY, TSX,
        TXA, TXS, TYA, XAA, XXX,
    },
    INSTRUCTIONS,
};
use log::{log_enabled, trace, Level};
use serde::{Deserialize, Serialize};
use std::fmt;

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
const POWER_ON_STATUS: Status = Status::U.union(Status::I);
const SP_BASE: u16 = 0x0100; // Stack-pointer starting address

bitflags! {
    #[derive(Default, Serialize, Deserialize)]
    #[must_use]
    pub struct Irq: u8 {
        const RESET = 1;
        const MAPPER = 1 << 1;
        const FRAME_COUNTER = 1 << 2;
        const DMC = 1 << 3;
    }
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
bitflags! {
    #[derive(Default, Serialize, Deserialize)]
    #[must_use]
    pub struct Status: u8 {
        const C = 1;      // Carry
        const Z = 1 << 1; // Zero
        const I = 1 << 2; // Disable Interrupt
        const D = 1 << 3; // Decimal Mode
        const B = 1 << 4; // Break
        const U = 1 << 5; // Unused
        const V = 1 << 6; // Overflow
        const N = 1 << 7; // Negative
    }
}
pub const STATUS_REGS: [Status; 8] = [
    Status::N,
    Status::V,
    Status::U,
    Status::B,
    Status::D,
    Status::I,
    Status::Z,
    Status::C,
];

/// The Central Processing Unit status and registers
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Cpu {
    pub cycle_count: usize, // total number of cycles ran
    pub step: usize,        // total number of CPU instructions run
    pub pc: u16,            // program counter
    pub sp: u8,             // stack pointer - stack is at $0100-$01FF
    pub acc: u8,            // accumulator
    pub x: u8,              // x register
    pub y: u8,              // y register
    pub status: Status,     // Status Registers
    pub bus: Bus,
    pub instr: Instr,      // The currently executing instruction
    pub abs_addr: u16,     // Used memory addresses get set here
    pub rel_addr: u16,     // Relative address for branch instructions
    pub fetched_data: u8,  // Represents data fetched for the ALU
    pub irqs_pending: Irq, // Pending interrupts
    pub nmi_pending: bool,
    #[serde(skip)]
    pub corrupted: bool, // Encountering an invalid opcode corrupts CPU processing
    pub last_irq: bool,
    pub last_nmi: bool,
    pub dmc_dma: bool,
    #[serde(skip)]
    pub debugging: bool,
}

impl Cpu {
    pub const fn init(bus: Bus) -> Self {
        Self {
            cycle_count: 0,
            step: 0,
            pc: 0x0000,
            sp: 0x00,
            acc: 0x00,
            x: 0x00,
            y: 0x00,
            status: POWER_ON_STATUS,
            bus,
            instr: INSTRUCTIONS[0x00],
            abs_addr: 0x0000,
            rel_addr: 0x0000,
            fetched_data: 0x00,
            irqs_pending: Irq::RESET,
            nmi_pending: false,
            corrupted: false,
            last_irq: false,
            last_nmi: false,
            dmc_dma: false,
            debugging: false,
        }
    }

    #[inline]
    pub fn next_instr(&self) -> Instr {
        let opcode = self.peek(self.pc);
        INSTRUCTIONS[opcode as usize]
    }

    #[inline]
    #[must_use]
    pub fn next_addr(&self) -> (Option<u16>, Option<u16>) {
        let instr = self.next_instr();
        let addr = self.pc.wrapping_add(1);
        let (addr, val) = match instr.addr_mode() {
            IMM => (None, Some(self.peek(addr).into())),
            ZP0 => {
                let abs_addr = self.peek(addr);
                let val = self.peek(abs_addr.into());
                (Some(abs_addr.into()), Some(val.into()))
            }
            ZPX => {
                let abs_addr = self.peek(addr);
                let x_offset = abs_addr.wrapping_add(self.x);
                let val = self.peek(x_offset.into());
                (Some(x_offset.into()), Some(val.into()))
            }
            ZPY => {
                let abs_addr = self.peek(addr);
                let y_offset = abs_addr.wrapping_add(self.y);
                let val = self.peek(y_offset.into());
                (Some(y_offset.into()), Some(val.into()))
            }
            ABS => {
                let abs_addr = self.peekw(addr);
                if instr.op() == JMP || instr.op() == JSR {
                    (Some(abs_addr), None)
                } else {
                    let val = self.peek(abs_addr);
                    (Some(abs_addr), Some(val.into()))
                }
            }
            ABX => {
                let abs_addr = self.peekw(addr);
                let x_offset = abs_addr.wrapping_add(self.x.into());
                let val = self.peek(x_offset);
                (Some(x_offset), Some(val.into()))
            }
            ABY => {
                let abs_addr = self.peekw(addr);
                let y_offset = abs_addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset);
                (Some(y_offset), Some(val.into()))
            }
            IND => {
                let abs_addr = self.peekw(addr);
                let val = if abs_addr & 0x00FF == 0x00FF {
                    (u16::from(self.peek(abs_addr & 0xFF00)) << 8) | u16::from(self.peek(abs_addr))
                } else {
                    (u16::from(self.peek(abs_addr + 1)) << 8) | u16::from(self.peek(abs_addr))
                };
                (Some(abs_addr), Some(val))
            }
            IDX => {
                let ind_addr = self.peek(addr);
                let x_offset = ind_addr.wrapping_add(self.x);
                let abs_addr = self.peekw_zp(x_offset);
                let val = self.peek(abs_addr);
                (Some(abs_addr), Some(val.into()))
            }
            IDY => {
                let ind_addr = self.peek(addr);
                let abs_addr = self.peekw_zp(ind_addr);
                let y_offset = abs_addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset);
                (Some(y_offset), Some(val.into()))
            }
            REL => {
                let mut rel_addr = self.peek(addr).into();
                if rel_addr & 0x80 == 0x80 {
                    // If address is negative, extend sign to 16-bits
                    rel_addr |= 0xFF00;
                }
                (Some(rel_addr), None)
            }
            ACC => (None, Some(self.acc.into())),
            IMP => match instr.op() {
                TXA | TYA => (None, Some(self.acc.into())),
                INY | DEY | TAY => (None, Some(self.y.into())),
                INX | DEX | TAX | TSX => (None, Some(self.x.into())),
                TXS => (None, Some(self.sp.into())),
                _ => (None, None),
            },
        };
        (addr, val)
    }

    /// Sends an IRQ Interrupt to the CPU
    ///
    /// <http://wiki.nesdev.com/w/index.php/IRQ>
    #[inline]
    pub fn set_irq(&mut self, irq: Irq, val: bool) {
        if val {
            self.irqs_pending |= irq;
        } else {
            self.irqs_pending &= !irq;
        }
    }

    /// Checks if a a given IRQ is active
    #[inline]
    pub fn has_irq(&mut self, irq: Irq) -> bool {
        self.irqs_pending.intersects(irq)
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
    #[inline]
    pub fn irq(&mut self) {
        self.read(self.pc);
        self.read(self.pc);
        if self.has_irq(Irq::RESET) {
            // EXPL: Reset follows the same push behavior as IRQ/NMI except the
            // read flag is set, so results are discarded
            self.push_stackw_nop();
            self.push_stackb_nop();
            self.pc = self.readw(RESET_ADDR);
            self.set_irq(Irq::RESET, false);
        } else {
            self.push_stackw(self.pc);
            // Set U and !B during push
            self.push_stackb(((self.status | Status::U) & !Status::B).bits());
            self.status.set(Status::I, true);
            if self.has_irq(Irq::RESET) {
                self.pc = self.readw(RESET_ADDR);
                self.set_irq(Irq::RESET, false);
            } else if self.last_nmi {
                self.nmi_pending = false;
                self.bus.ppu.nmi_pending = false;
                self.pc = self.readw(NMI_ADDR);
            } else {
                self.pc = self.readw(IRQ_ADDR);
            }
            // Prevent NMI from triggering immediately after IRQ
            self.last_nmi = false;
        }
    }

    /// Sends a NMI Interrupt to the CPU
    ///
    /// <http://wiki.nesdev.com/w/index.php/NMI>
    #[inline]
    pub fn set_nmi(&mut self, val: bool) {
        self.nmi_pending = val;
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
    #[inline]
    fn nmi(&mut self) {
        self.read(self.pc);
        self.read(self.pc);
        self.push_stackw(self.pc);
        // Set U and !B during push
        self.push_stackb(((self.status | Status::U) & !Status::B).bits());
        self.status.set(Status::I, true);
        self.pc = self.readw(NMI_ADDR);
        self.nmi_pending = false;
        self.bus.ppu.nmi_pending = false;
    }

    #[inline]
    fn run_cycle(&mut self) {
        self.cycle_count = self.cycle_count.wrapping_add(1);
        self.last_nmi = self.nmi_pending;
        self.last_irq = !self.irqs_pending.is_empty() && !self.status.intersects(Status::I);
        self.bus.ppu.clock();
        self.set_nmi(self.bus.ppu.nmi_pending);
        self.bus.cart.clock();
        let irq_pending = self.bus.cart.irq_pending();
        self.set_irq(Irq::MAPPER, irq_pending);
        self.bus.apu.clock();
        self.set_irq(Irq::FRAME_COUNTER, self.bus.apu.irq_pending);
        self.set_irq(Irq::DMC, self.bus.apu.dmc.irq_pending);
        if self.bus.apu.dmc.dma_pending {
            self.bus.apu.dmc.dma_pending = false;
            self.dmc_dma = true;
            self.bus.halt = true;
            self.bus.dummy_read = true;
        }
    }

    #[inline]
    fn process_dma_cycle(&mut self) {
        // OAM DMA cycles count as halt/dummy reads for DMC DMA when both run at the same time
        if self.bus.halt {
            self.bus.halt = false;
        } else if self.bus.dummy_read {
            self.bus.dummy_read = false;
        }
        self.run_cycle();
    }

    #[inline]
    fn handle_dma(&mut self, addr: u16) {
        if !self.bus.halt {
            return;
        }

        self.run_cycle();
        self.bus.read(addr);
        self.bus.halt = false;

        let skip_dummy_reads = addr == 0x4016 || addr == 0x4017;
        let oam_read_addr = u16::from(self.bus.ppu.dma_offset) << 8;
        let mut oam_read_offset = 0;
        let mut oam_data = 0;
        let mut oam_dma_count = 0;

        while self.bus.ppu.oam_dma || self.dmc_dma {
            if self.cycle_count & 0x01 == 0x00 {
                if self.dmc_dma && !self.bus.halt && !self.bus.dummy_read {
                    // DMC DMA ready to read a byte (halt and dummy read done before)
                    self.process_dma_cycle();
                    let val = self.bus.read(self.bus.apu.dmc.addr);
                    self.bus.apu.dmc.set_sample_buffer(val);
                    self.dmc_dma = false;
                } else if self.bus.ppu.oam_dma {
                    // DMC DMA not running or ready, run OAM DMA
                    self.process_dma_cycle();
                    oam_data = self.bus.read(oam_read_addr + oam_read_offset);
                    oam_read_offset += 1;
                    oam_dma_count += 1;
                } else {
                    // DMC DMA running, but not ready yet (needs to halt, or dummy read) and OAM
                    // DMA isn't running
                    debug_assert!(self.bus.halt || self.bus.dummy_read);
                    self.process_dma_cycle();
                    if !skip_dummy_reads {
                        self.bus.read(addr); // throw away
                    }
                }
            } else if self.bus.ppu.oam_dma && oam_dma_count & 0x01 == 0x01 {
                // OAM DMA write cycle, done on odd cycles after a read on even cycles
                self.process_dma_cycle();
                self.bus.write(0x2004, oam_data);
                oam_dma_count += 1;
                if oam_dma_count == 0x200 {
                    // Finished OAM DMA
                    self.bus.ppu.oam_dma = false;
                }
            } else {
                // Align to read cycle before starting OAM DMA (or align to perform DMC read)
                self.process_dma_cycle();
                if !skip_dummy_reads {
                    self.bus.read(addr); // throw away
                }
            }
        }
    }

    // Status Register functions

    // Convenience method to set both Z and N
    #[inline]
    fn set_zn_status(&mut self, val: u8) {
        self.status.set(Status::Z, val == 0x00);
        self.status.set(Status::N, val & 0x80 == 0x80);
    }

    #[inline]
    const fn status_bit(&self, reg: Status) -> u8 {
        self.status.intersection(reg).bits()
    }

    // Stack Functions

    // Push a byte to the stack
    #[inline]
    fn push_stackb(&mut self, val: u8) {
        self.write(SP_BASE | u16::from(self.sp), val);
        self.sp = self.sp.wrapping_sub(1);
    }

    // Push a byte to the stack with read value set, so no actual operation is done
    // except decrement the stack pointer
    // Used by Irq::Reset
    #[inline]
    fn push_stackb_nop(&mut self) {
        let _ = self.read(SP_BASE | u16::from(self.sp));
        self.sp = self.sp.wrapping_sub(1);
    }

    // Pull a byte from the stack
    #[must_use]
    #[inline]
    fn pop_stackb(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.read(SP_BASE | u16::from(self.sp))
    }

    // Peek byte at the top of the stack
    #[must_use]
    #[inline]
    pub fn peek_stackb(&self) -> u8 {
        self.peek(SP_BASE | u16::from(self.sp.wrapping_add(1)))
    }

    // Push a word (two bytes) to the stack
    #[inline]
    fn push_stackw(&mut self, val: u16) {
        let lo = (val & 0xFF) as u8;
        let hi = (val >> 8) as u8;
        self.push_stackb(hi);
        self.push_stackb(lo);
    }

    // Push a word (two bytes) to the stack with read value set, so no actual operation is done
    // except decrementing the stack pointer
    // Used by Irq::Reset
    #[inline]
    fn push_stackw_nop(&mut self) {
        self.push_stackb_nop();
        self.push_stackb_nop();
    }

    // Pull a word (two bytes) from the stack
    #[inline]
    fn pop_stackw(&mut self) -> u16 {
        let lo = u16::from(self.pop_stackb());
        let hi = u16::from(self.pop_stackb());
        hi << 8 | lo
    }

    // Peek at the top of the stack
    #[must_use]
    #[inline]
    pub fn peek_stackw(&self) -> u16 {
        let lo = u16::from(self.peek(SP_BASE | u16::from(self.sp)));
        let sp = self.sp.wrapping_add(1);
        let hi = u16::from(self.peek(SP_BASE | u16::from(sp)));
        hi << 8 | lo
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
                            _ => unreachable!("not possible"),
                        };
                        // Read if we crossed, otherwise use what was already set in cycle 4 from
                        // addressing mode
                        //
                        // ABX/ABY/IDY all add `reg` to `abs_addr`, so this checks if it wrapped
                        // around to 0.
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
    #[inline]
    fn write_fetched(&mut self, val: u8) {
        match self.instr.addr_mode() {
            IMP | ACC => self.acc = val,
            IMM => (), // noop
            _ => self.write(self.abs_addr, val),
        }
    }

    // Memory accesses

    // Utility to read a full 16-bit word
    #[must_use]
    #[inline]
    pub fn readw(&mut self, addr: u16) -> u16 {
        let lo = u16::from(self.read(addr));
        let hi = u16::from(self.read(addr.wrapping_add(1)));
        (hi << 8) | lo
    }

    // readw but don't accidentally modify state
    #[must_use]
    #[inline]
    pub fn peekw(&self, addr: u16) -> u16 {
        let lo = u16::from(self.peek(addr));
        let hi = u16::from(self.peek(addr.wrapping_add(1)));
        (hi << 8) | lo
    }

    // Like readw, but for Zero Page which means it'll wrap around at 0xFF
    #[must_use]
    #[inline]
    fn readw_zp(&mut self, addr: u8) -> u16 {
        let lo = u16::from(self.read(addr.into()));
        let hi = u16::from(self.read(addr.wrapping_add(1).into()));
        (hi << 8) | lo
    }

    // Like peekw, but for Zero Page which means it'll wrap around at 0xFF
    #[must_use]
    #[inline]
    fn peekw_zp(&self, addr: u8) -> u16 {
        let lo = u16::from(self.peek(addr.into()));
        let hi = u16::from(self.peek(addr.wrapping_add(1).into()));
        (hi << 8) | lo
    }

    #[must_use]
    pub fn disassemble(&self, pc: &mut u16) -> String {
        let opcode = self.peek(*pc);
        let instr = INSTRUCTIONS[opcode as usize];
        let mut bytes = Vec::with_capacity(3);
        let mut disasm = String::with_capacity(100);
        disasm.push_str(&format!("${:04X} ", pc));
        bytes.push(opcode);
        let mut addr = pc.wrapping_add(1);
        let mode = match instr.addr_mode() {
            IMM => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                format!(" #${:02X}", bytes[1])
            }
            ZP0 => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                let val = self.peek(bytes[1].into());
                format!(" ${:02X} = ${:02X}", bytes[1], val)
            }
            ZPX => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                let x_offset = bytes[1].wrapping_add(self.x);
                let val = self.peek(x_offset.into());
                format!(" ${:02X},X @ ${:02X} = ${:02X}", bytes[1], x_offset, val)
            }
            ZPY => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                let y_offset = bytes[1].wrapping_add(self.y);
                let val = self.peek(y_offset.into());
                format!(" ${:02X},Y @ ${:02X} = ${:02X}", bytes[1], y_offset, val)
            }
            ABS => {
                bytes.push(self.peek(addr));
                bytes.push(self.peek(addr.wrapping_add(1)));
                let abs_addr = self.peekw(addr);
                addr = addr.wrapping_add(2);
                if instr.op() == JMP || instr.op() == JSR {
                    format!(" ${:04X}", abs_addr)
                } else {
                    let val = self.peek(abs_addr);
                    format!(" ${:04X} = ${:02X}", abs_addr, val)
                }
            }
            ABX => {
                bytes.push(self.peek(addr));
                bytes.push(self.peek(addr.wrapping_add(1)));
                let abs_addr = self.peekw(addr);
                addr = addr.wrapping_add(2);
                let x_offset = abs_addr.wrapping_add(self.x.into());
                let val = self.peek(x_offset);
                format!(" ${:04X},X @ ${:04X} = ${:02X}", abs_addr, x_offset, val)
            }
            ABY => {
                bytes.push(self.peek(addr));
                bytes.push(self.peek(addr.wrapping_add(1)));
                let abs_addr = self.peekw(addr);
                addr = addr.wrapping_add(2);
                let y_offset = abs_addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset);
                format!(" ${:04X},Y @ ${:04X} = ${:02X}", abs_addr, y_offset, val)
            }
            IND => {
                bytes.push(self.peek(addr));
                bytes.push(self.peek(addr.wrapping_add(1)));
                let abs_addr = self.peekw(addr);
                addr = addr.wrapping_add(2);
                let val = if abs_addr & 0x00FF == 0x00FF {
                    (u16::from(self.peek(abs_addr & 0xFF00)) << 8) | u16::from(self.peek(abs_addr))
                } else {
                    (u16::from(self.peek(abs_addr + 1)) << 8) | u16::from(self.peek(abs_addr))
                };
                format!(" (${:04X}) = ${:04X}", abs_addr, val)
            }
            IDX => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                let x_offset = bytes[1].wrapping_add(self.x);
                let abs_addr = self.peekw_zp(x_offset);
                let val = self.peek(abs_addr);
                format!(" (${:02X},X) @ ${:04X} = ${:02X}", bytes[1], abs_addr, val)
            }
            IDY => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                let abs_addr = self.peekw_zp(bytes[1]);
                let y_offset = abs_addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset);
                format!(" (${:02X}),Y @ ${:04X} = ${:02X}", bytes[1], y_offset, val)
            }
            REL => {
                bytes.push(self.peek(addr));
                let mut rel_addr = self.peek(addr).into();
                addr = addr.wrapping_add(1);
                if rel_addr & 0x80 == 0x80 {
                    // If address is negative, extend sign to 16-bits
                    rel_addr |= 0xFF00;
                }
                format!(" ${:04X}", addr.wrapping_add(rel_addr))
            }
            ACC | IMP => "".to_string(),
        };
        *pc = addr;
        for i in 0..3 {
            if i < bytes.len() {
                disasm.push_str(&format!("${:02X} ", bytes[i]));
            } else {
                disasm.push_str("    ");
            }
        }
        disasm.push_str(&format!("{:?}{}", instr, mode));
        disasm
    }

    // Print the current instruction and status
    pub fn print_instruction(&self) {
        let mut pc = self.pc;
        let disasm = self.disassemble(&mut pc);

        let flags = ['n', 'v', '-', '-', 'd', 'i', 'z', 'c'];
        let mut status_str = String::with_capacity(8);
        for (flag, status) in flags.iter().zip(STATUS_REGS.iter()) {
            if self.status.intersects(*status) {
                status_str.push(flag.to_ascii_uppercase());
            } else {
                status_str.push(*flag);
            }
        }
        trace!(
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

    #[must_use]
    #[inline]
    const fn pages_differ(addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }
}

impl Clocked for Cpu {
    /// Runs the CPU one instruction
    fn clock(&mut self) -> usize {
        let start_cycles = self.cycle_count;

        if self.has_irq(Irq::RESET) {
            self.irq();
        } else if self.last_nmi {
            self.nmi();
        } else if self.last_irq {
            self.irq();
        }

        let opcode = self.read(self.pc); // Cycle 1 of instruction
        if log_enabled!(Level::Trace) && self.debugging {
            self.print_instruction();
        }
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
        }

        self.step += 1;
        self.cycle_count - start_cycles
    }
}

impl MemRead for Cpu {
    #[inline]
    fn read(&mut self, addr: u16) -> u8 {
        self.handle_dma(addr);
        self.run_cycle();
        self.bus.read(addr)
    }

    #[inline]
    fn peek(&self, addr: u16) -> u8 {
        self.bus.peek(addr)
    }
}
impl MemWrite for Cpu {
    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        self.run_cycle();
        self.bus.write(addr, val);
    }
}

impl Powered for Cpu {
    /// Powers on the CPU
    fn power_on(&mut self) {
        self.cycle_count = 0;
        self.dmc_dma = false;
        self.set_irq(Irq::RESET, true);
    }

    /// Resets the CPU
    ///
    /// Updates the PC, SP, and Status values to defined constants.
    ///
    /// These operations take the CPU 7 cycle.
    fn reset(&mut self) {
        self.bus.reset();
        self.status.set(Status::I, true);
        self.power_on();
    }

    /// Power cycle the CPU
    ///
    /// Updates all status as if powered on for the first time
    ///
    /// These operations take the CPU 7 cycle.
    fn power_cycle(&mut self) {
        self.bus.power_cycle();
        self.acc = 0x00;
        self.x = 0x00;
        self.y = 0x00;
        self.status = POWER_ON_STATUS;
        self.sp = 0x00;
        self.power_on();
    }
}

impl fmt::Debug for Cpu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
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
    #[test]
    fn cpu_cycle_timing() {
        use super::*;
        use crate::memory::RamState;
        let mut cpu = Cpu::init(Bus::new(RamState::AllZeros));
        cpu.power_on();
        cpu.clock();

        assert_eq!(cpu.cycle_count, 14, "cpu after power + one clock");
        assert_eq!(cpu.bus.ppu.cycle_count, 42, "ppu after power + one clock");

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
            cpu.bus.wram.write(0x0000, instr.opcode());
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
                ppu_cyc,
                "ppu ${:02X} {:?} #{:?}",
                instr.opcode(),
                instr.op(),
                instr.addr_mode()
            );
        }
    }
}
