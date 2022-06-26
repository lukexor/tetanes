//! A 6502 Central Processing Unit
//!
//! <http://wiki.nesdev.com/w/index.php/CPU>

use crate::{
    bus::Bus,
    common::{Clock, Kind, NesRegion, Reset},
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
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write};

pub mod instr;

// Represents CPU/PPU alignment and would range from 0..=ppu_divider-1, if random alignment was emulated
const PPU_OFFSET: u64 = 1;

const NMI_VECTOR: u16 = 0xFFFA; // NMI Vector address
const IRQ_VECTOR: u16 = 0xFFFE; // IRQ Vector address
const RESET_VECTOR: u16 = 0xFFFC; // Vector address at reset
const POWER_ON_STATUS: Status = Status::U.union(Status::I);
const POWER_ON_SP: u8 = 0xFD;
const SP_BASE: u16 = 0x0100; // Stack-pointer starting address

bitflags! {
    #[derive(Default, Serialize, Deserialize)]
    #[must_use]
    pub struct Irq: u8 {
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

/// Every cycle is either a read or a write.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
enum Cycle {
    Read,
    Write,
}

/// The Central Processing Unit status and registers
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Cpu {
    pub cycle: usize, // total number of cycles ran
    pub step: usize,  // total number of CPU instructions run
    pub region: NesRegion,
    pub master_clock: u64,
    pub clock_divider: u64,
    pub start_clocks: u64,
    pub end_clocks: u64,
    pub pc: u16,        // program counter
    pub sp: u8,         // stack pointer - stack is at $0100-$01FF
    pub acc: u8,        // accumulator
    pub x: u8,          // x register
    pub y: u8,          // y register
    pub status: Status, // Status Registers
    pub bus: Bus,
    pub instr: Instr,     // The currently executing instruction
    pub abs_addr: u16,    // Used memory addresses get set here
    pub rel_addr: u16,    // Relative address for branch instructions
    pub fetched_data: u8, // Represents data fetched for the ALU
    pub irq: Irq,         // Pending interrupts
    pub run_irq: bool,
    pub prev_run_irq: bool,
    pub nmi: bool,
    pub prev_nmi: bool,
    pub prev_nmi_pending: bool,
    #[serde(skip)]
    pub corrupted: bool, // Encountering an invalid opcode corrupts CPU processing
    pub halt: bool,
    pub dmc_dma: bool,
    pub dummy_read: bool,
    #[serde(skip)]
    pub debugging: bool,
}

impl Cpu {
    // TODO 1789772.667 MHz (~559 ns/cycle) - May want to use 1786830 for a stable 60 FPS
    // Add Emulator setting like Mesen??
    // http://forums.nesdev.com/viewtopic.php?p=223679#p223679
    const NTSC_MASTER_CLOCK_RATE: f32 = 21_477_272.0;
    const NTSC_CPU_CLOCK_RATE: f32 = Self::NTSC_MASTER_CLOCK_RATE / 12.0;
    const PAL_MASTER_CLOCK_RATE: f32 = 26_601_712.0;
    const PAL_CPU_CLOCK_RATE: f32 = Self::PAL_MASTER_CLOCK_RATE / 16.0;
    const DENDY_CPU_CLOCK_RATE: f32 = Self::PAL_MASTER_CLOCK_RATE / 15.0;

    pub fn new(bus: Bus) -> Self {
        let mut cpu = Self {
            cycle: 0,
            step: 0,
            region: NesRegion::default(),
            master_clock: 0,
            clock_divider: 0,
            start_clocks: 0,
            end_clocks: 0,
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
            irq: Irq::empty(),
            run_irq: false,
            prev_run_irq: false,
            nmi: false,
            prev_nmi: false,
            prev_nmi_pending: false,
            corrupted: false,
            halt: false,
            dmc_dma: false,
            dummy_read: false,
            debugging: false,
        };
        cpu.set_region(cpu.region);
        cpu.reset(Kind::Hard);
        cpu
    }

    #[inline]
    #[must_use]
    pub const fn region_clock_rate(region: NesRegion) -> f32 {
        match region {
            NesRegion::Ntsc => Self::NTSC_CPU_CLOCK_RATE,
            NesRegion::Pal => Self::PAL_CPU_CLOCK_RATE,
            NesRegion::Dendy => Self::DENDY_CPU_CLOCK_RATE,
        }
    }

    #[inline]
    #[must_use]
    pub const fn clock_rate(&self) -> f32 {
        Self::region_clock_rate(self.region)
    }

    #[inline]
    pub fn region(&self) -> NesRegion {
        self.region
    }

    pub fn set_region(&mut self, region: NesRegion) {
        let (clock_divider, start_clocks, end_clocks) = match region {
            NesRegion::Ntsc => (12, 6, 6),
            NesRegion::Pal => (16, 8, 8),
            NesRegion::Dendy => (15, 7, 8),
        };
        self.region = region;
        self.clock_divider = clock_divider;
        self.start_clocks = start_clocks;
        self.end_clocks = end_clocks;
        self.bus.set_region(region);
    }

    #[inline]
    pub fn next_instr(&self) -> Instr {
        let opcode = self.peek(self.pc);
        INSTRUCTIONS[opcode as usize]
    }

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
                let abs_addr = self.peek_word(addr);
                if instr.op() == JMP || instr.op() == JSR {
                    (Some(abs_addr), None)
                } else {
                    let val = self.peek(abs_addr);
                    (Some(abs_addr), Some(val.into()))
                }
            }
            ABX => {
                let abs_addr = self.peek_word(addr);
                let x_offset = abs_addr.wrapping_add(self.x.into());
                let val = self.peek(x_offset);
                (Some(x_offset), Some(val.into()))
            }
            ABY => {
                let abs_addr = self.peek_word(addr);
                let y_offset = abs_addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset);
                (Some(y_offset), Some(val.into()))
            }
            IND => {
                let abs_addr = self.peek_word(addr);
                let lo = self.peek(abs_addr);
                let hi = if abs_addr & 0x00FF == 0x00FF {
                    self.peek(abs_addr & 0xFF00)
                } else {
                    self.peek(abs_addr + 1)
                };
                let val = u16::from_le_bytes([lo, hi]);
                (Some(abs_addr), Some(val))
            }
            IDX => {
                let ind_addr = self.peek(addr);
                let x_offset = ind_addr.wrapping_add(self.x);
                let abs_addr = self.peek_word_zp(x_offset);
                let val = self.peek(abs_addr);
                (Some(abs_addr), Some(val.into()))
            }
            IDY => {
                let ind_addr = self.peek(addr);
                let abs_addr = self.peek_word_zp(ind_addr);
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

    // <http://wiki.nesdev.com/w/index.php/IRQ>
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
        self.push_word(self.pc);

        // Pushing status to the stack has to happen after checking NMI since it can hijack the BRK
        // IRQ when it occurs between cycles 4 and 5.
        // https://www.nesdev.org/wiki/CPU_interrupts#Interrupt_hijacking
        //
        // Set U and !B during push
        let status = ((self.status | Status::U) & !Status::B).bits();

        if self.nmi {
            self.nmi = false;
            self.push(status);
            self.status.set(Status::I, true);

            self.pc = self.read_word(NMI_VECTOR);
            if self.debugging {
                log::trace!("NMI: {}", self.cycle);
            }
        } else {
            self.push(status);
            self.status.set(Status::I, true);

            self.pc = self.read_word(IRQ_VECTOR);
            if self.debugging {
                log::trace!("IRQ: {}", self.cycle);
            }
        }
    }

    fn handle_interrupts(&mut self) {
        // https://www.nesdev.org/wiki/CPU_interrupts
        //
        // The internal signal goes high during φ1 of the cycle that follows the one where
        // the edge is detected, and stays high until the NMI has been handled. NMI is handled only
        // when `prev_nmi` is true.
        self.prev_nmi = self.nmi;

        // This edge detector polls the status of the NMI line during φ2 of each CPU cycle (i.e.,
        // during the second half of each cycle, hence here in `end_cycle`) and raises an internal
        // signal if the input goes from being high during one cycle to being low during the
        // next.
        let nmi_pending = self.bus.ppu.nmi_pending();
        if !self.prev_nmi_pending && nmi_pending {
            self.nmi = true;
            log::trace!("NMI Edge Detected: {}, {}", self.nmi, self.cycle);
        }
        self.prev_nmi_pending = nmi_pending;

        self.irq.set(Irq::MAPPER, self.bus.cart.irq_pending());
        self.irq.set(Irq::FRAME_COUNTER, self.bus.apu.irq_pending);
        self.irq.set(Irq::DMC, self.bus.apu.dmc.irq_pending);

        // The IRQ status at the end of the second-to-last cycle is what matters,
        // so keep the second-to-last status.
        self.prev_run_irq = self.run_irq;
        self.run_irq = !self.irq.is_empty() && !self.status.intersects(Status::I);
        if self.run_irq {
            log::trace!("IRQ Level Detected: {}, {}", self.run_irq, self.cycle);
        }

        if self.bus.apu.dmc.dma_pending {
            self.bus.apu.dmc.dma_pending = false;
            self.dmc_dma = true;
            self.halt = true;
            self.dummy_read = true;
        }
    }

    fn start_cycle(&mut self, cycle: Cycle) {
        self.master_clock += if cycle == Cycle::Read {
            self.start_clocks - 1
        } else {
            self.start_clocks + 1
        };
        self.cycle = self.cycle.wrapping_add(1);

        #[cfg(feature = "cycle-accurate")]
        {
            self.bus.ppu.clock_to(self.master_clock - PPU_OFFSET);
            self.bus.cart.clock();
            self.bus.apu.clock();
        }
    }

    fn end_cycle(&mut self, cycle: Cycle) {
        self.master_clock += if cycle == Cycle::Read {
            self.end_clocks + 1
        } else {
            self.end_clocks - 1
        };

        #[cfg(feature = "cycle-accurate")]
        {
            self.bus.ppu.clock_to(self.master_clock - PPU_OFFSET);
        }

        self.handle_interrupts();
    }

    fn process_dma_cycle(&mut self) {
        // OAM DMA cycles count as halt/dummy reads for DMC DMA when both run at the same time
        if self.halt {
            self.halt = false;
        } else if self.dummy_read {
            self.dummy_read = false;
        }
        self.start_cycle(Cycle::Read);
    }

    fn handle_dma(&mut self, addr: u16) {
        self.start_cycle(Cycle::Read);
        self.bus.read(addr);
        self.end_cycle(Cycle::Read);
        self.halt = false;

        let skip_dummy_reads = addr == 0x4016 || addr == 0x4017;

        let oam_read_addr = u16::from(self.bus.ppu.oam_dma_offset) << 8;
        let mut oam_read_offset = 0;
        let mut oam_dma_count = 0;
        let mut read_val = 0;

        while self.bus.ppu.oam_dma || self.dmc_dma {
            if self.cycle & 0x01 == 0x00 {
                if self.dmc_dma && !self.halt && !self.dummy_read {
                    // DMC DMA ready to read a byte (halt and dummy read done before)
                    self.process_dma_cycle();
                    read_val = self.bus.read(self.bus.apu.dmc.addr);
                    self.end_cycle(Cycle::Read);
                    self.bus.apu.dmc.set_sample_buffer(read_val);
                    self.dmc_dma = false;
                } else if self.bus.ppu.oam_dma {
                    // DMC DMA not running or ready, run OAM DMA
                    self.process_dma_cycle();
                    read_val = self.bus.read(oam_read_addr + oam_read_offset);
                    self.end_cycle(Cycle::Read);
                    oam_read_offset += 1;
                    oam_dma_count += 1;
                } else {
                    // DMC DMA running, but not ready yet (needs to halt, or dummy read) and OAM
                    // DMA isn't running
                    debug_assert!(self.halt || self.dummy_read);
                    self.process_dma_cycle();
                    if !skip_dummy_reads {
                        self.bus.read(addr); // throw away
                    }
                    self.end_cycle(Cycle::Read);
                }
            } else if self.bus.ppu.oam_dma && oam_dma_count & 0x01 == 0x01 {
                // OAM DMA write cycle, done on odd cycles after a read on even cycles
                self.process_dma_cycle();
                self.bus.write(0x2004, read_val);
                self.end_cycle(Cycle::Read);
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
                self.end_cycle(Cycle::Read);
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
    fn push(&mut self, val: u8) {
        self.write(SP_BASE | u16::from(self.sp), val);
        self.sp = self.sp.wrapping_sub(1);
    }

    // Pull a byte from the stack
    #[must_use]
    #[inline]
    fn pop(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.read(SP_BASE | u16::from(self.sp))
    }

    // Peek byte at the top of the stack
    #[must_use]
    #[inline]
    pub fn peek_stack(&self) -> u8 {
        self.peek(SP_BASE | u16::from(self.sp.wrapping_add(1)))
    }

    // Peek at the top of the stack
    #[must_use]
    #[inline]
    pub fn peek_stack_word(&self) -> u16 {
        let lo = self.peek(SP_BASE | u16::from(self.sp));
        let hi = self.peek(SP_BASE | u16::from(self.sp.wrapping_add(1)));
        u16::from_le_bytes([lo, hi])
    }

    // Push a word (two bytes) to the stack
    #[inline]
    fn push_word(&mut self, val: u16) {
        let [lo, hi] = val.to_le_bytes();
        self.push(hi);
        self.push(lo);
    }

    // Pull a word (two bytes) from the stack
    #[inline]
    fn pop_word(&mut self) -> u16 {
        let lo = self.pop();
        let hi = self.pop();
        u16::from_le_bytes([lo, hi])
    }

    // Memory accesses

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

    // Reads an instruction byte and increments PC by 1.
    #[must_use]
    #[inline]
    fn read_instr_byte(&mut self) -> u8 {
        let val = self.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        val
    }

    // Reads an instruction 16-bit word and increments PC by 2.
    #[must_use]
    #[inline]
    fn read_instr_word(&mut self) -> u16 {
        let lo = self.read_instr_byte();
        let hi = self.read_instr_byte();
        u16::from_le_bytes([lo, hi])
    }

    // Read a 16-bit word.
    #[must_use]
    #[inline]
    pub fn read_word(&mut self, addr: u16) -> u16 {
        let lo = self.read(addr);
        let hi = self.read(addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    // Peek a 16-bit word without side effects.
    #[must_use]
    #[inline]
    pub fn peek_word(&self, addr: u16) -> u16 {
        let lo = self.peek(addr);
        let hi = self.peek(addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    // Like read_word, but for Zero Page which means it'll wrap around at 0xFF
    #[must_use]
    #[inline]
    fn read_word_zp(&mut self, addr: u8) -> u16 {
        let lo = self.read(addr.into());
        let hi = self.read(addr.wrapping_add(1).into());
        u16::from_le_bytes([lo, hi])
    }

    // Like peek_word, but for Zero Page which means it'll wrap around at 0xFF
    #[must_use]
    #[inline]
    fn peek_word_zp(&self, addr: u8) -> u16 {
        let lo = self.peek(addr.into());
        let hi = self.peek(addr.wrapping_add(1).into());
        u16::from_le_bytes([lo, hi])
    }

    #[must_use]
    pub fn disassemble(&self, pc: &mut u16) -> String {
        let opcode = self.peek(*pc);
        let instr = INSTRUCTIONS[opcode as usize];
        let mut bytes = Vec::with_capacity(3);
        let mut disasm = String::with_capacity(100);
        let _ = write!(disasm, "{:04X} ", pc);
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
                format!(" ${:02X} = #${:02X}", bytes[1], val)
            }
            ZPX => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                let x_offset = bytes[1].wrapping_add(self.x);
                let val = self.peek(x_offset.into());
                format!(" ${:02X},X @ ${:02X} = #${:02X}", bytes[1], x_offset, val)
            }
            ZPY => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                let y_offset = bytes[1].wrapping_add(self.y);
                let val = self.peek(y_offset.into());
                format!(" ${:02X},Y @ ${:02X} = #${:02X}", bytes[1], y_offset, val)
            }
            ABS => {
                bytes.push(self.peek(addr));
                bytes.push(self.peek(addr.wrapping_add(1)));
                let abs_addr = self.peek_word(addr);
                addr = addr.wrapping_add(2);
                if instr.op() == JMP || instr.op() == JSR {
                    format!(" ${:04X}", abs_addr)
                } else {
                    let val = self.peek(abs_addr);
                    format!(" ${:04X} = #${:02X}", abs_addr, val)
                }
            }
            ABX => {
                bytes.push(self.peek(addr));
                bytes.push(self.peek(addr.wrapping_add(1)));
                let abs_addr = self.peek_word(addr);
                addr = addr.wrapping_add(2);
                let x_offset = abs_addr.wrapping_add(self.x.into());
                let val = self.peek(x_offset);
                format!(" ${:04X},X @ ${:04X} = #${:02X}", abs_addr, x_offset, val)
            }
            ABY => {
                bytes.push(self.peek(addr));
                bytes.push(self.peek(addr.wrapping_add(1)));
                let abs_addr = self.peek_word(addr);
                addr = addr.wrapping_add(2);
                let y_offset = abs_addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset);
                format!(" ${:04X},Y @ ${:04X} = #${:02X}", abs_addr, y_offset, val)
            }
            IND => {
                bytes.push(self.peek(addr));
                bytes.push(self.peek(addr.wrapping_add(1)));
                let abs_addr = self.peek_word(addr);
                addr = addr.wrapping_add(2);
                let lo = self.peek(abs_addr);
                let hi = if abs_addr & 0x00FF == 0x00FF {
                    self.peek(abs_addr & 0xFF00)
                } else {
                    self.peek(abs_addr + 1)
                };
                let val = u16::from_le_bytes([lo, hi]);
                format!(" (${:04X}) = ${:04X}", abs_addr, val)
            }
            IDX => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                let x_offset = bytes[1].wrapping_add(self.x);
                let abs_addr = self.peek_word_zp(x_offset);
                let val = self.peek(abs_addr);
                format!(" (${:02X},X) @ ${:04X} = #${:02X}", bytes[1], abs_addr, val)
            }
            IDY => {
                bytes.push(self.peek(addr));
                addr = addr.wrapping_add(1);
                let abs_addr = self.peek_word_zp(bytes[1]);
                let y_offset = abs_addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset);
                format!(" (${:02X}),Y @ ${:04X} = #${:02X}", bytes[1], y_offset, val)
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
        for byte in &bytes {
            let _ = write!(disasm, "{:02X} ", byte);
        }
        for _ in 0..(3 - bytes.len()) {
            disasm.push_str("   ");
        }
        let _ = write!(disasm, "{:?}{}", instr, mode);
        disasm
    }

    // Print the current instruction and status
    pub fn trace_instr(&self) {
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
        log::trace!(
            "{:<50} A:{:02X} X:{:02X} Y:{:02X} P:{} SP:{:02X} PPU:{:3},{:3} CYC:{}",
            disasm,
            self.acc,
            self.x,
            self.y,
            status_str,
            self.sp,
            self.bus.ppu.cycle,
            self.bus.ppu.scanline,
            self.cycle,
        );
    }

    /// Utilities

    #[must_use]
    #[inline]
    const fn pages_differ(addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }
}

impl Clock for Cpu {
    /// Runs the CPU one instruction
    fn clock(&mut self) -> usize {
        let start_cycle = self.cycle;

        if self.debugging {
            self.trace_instr();
        }

        let opcode = self.read_instr_byte(); // Cycle 1 of instruction
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

        if self.prev_run_irq || self.prev_nmi {
            self.irq();
        }

        self.step += 1;

        #[cfg(not(feature = "cycle-accurate"))]
        {
            self.bus.ppu.run(self.master_clock - PPU_OFFSET);
            let cycles = self.cycle - start_cycle;
            for _ in 0..cycles {
                self.bus.cart.clock();
            }
            for _ in 0..cycles {
                self.bus.apu.clock();
            }
            self.handle_interrupts();
        }

        self.cycle - start_cycle
    }
}

impl MemRead for Cpu {
    fn read(&mut self, addr: u16) -> u8 {
        if self.halt {
            self.handle_dma(addr);
        }

        self.start_cycle(Cycle::Read);
        let val = self.bus.read(addr);
        self.end_cycle(Cycle::Read);
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        self.bus.peek(addr)
    }
}
impl MemWrite for Cpu {
    fn write(&mut self, addr: u16, val: u8) {
        self.start_cycle(Cycle::Write);
        self.bus.write(addr, val);
        self.end_cycle(Cycle::Write);
        if addr == 0x4014 {
            self.halt = true;
        }
    }
}

impl Reset for Cpu {
    /// Resets the CPU
    ///
    /// Updates the PC, SP, and Status values to defined constants.
    ///
    /// These operations take the CPU 7 cycles.
    fn reset(&mut self, kind: Kind) {
        match kind {
            Kind::Soft => {
                self.status.set(Status::I, true);
                // Reset pushes to the stack similar to IRQ, but since the read bit is set, nothing is
                // written except the SP being decremented
                self.sp = self.sp.wrapping_sub(0x03);
            }
            Kind::Hard => {
                self.acc = 0x00;
                self.x = 0x00;
                self.y = 0x00;
                self.status = POWER_ON_STATUS;
                self.sp = POWER_ON_SP;
            }
        }

        self.bus.reset(kind);
        self.cycle = 0;
        self.master_clock = 0;
        self.irq = Irq::empty();
        self.run_irq = false;
        self.prev_run_irq = false;
        self.nmi = false;
        self.prev_nmi = false;
        self.prev_nmi_pending = false;
        self.corrupted = false;
        self.dmc_dma = false;
        self.halt = false;
        self.dummy_read = false;

        // Read directly from bus so as to not clock other components during reset
        let lo = self.bus.read(RESET_VECTOR);
        let hi = self.bus.read(RESET_VECTOR + 1);
        self.pc = u16::from_le_bytes([lo, hi]);

        for _ in 0..8 {
            self.start_cycle(Cycle::Read);
            self.end_cycle(Cycle::Read);
        }
    }
}

impl fmt::Debug for Cpu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Cpu")
            .field("cycle", &self.cycle)
            .field("step", &self.step)
            .field("pc", &format_args!("${:04X}", self.pc))
            .field("sp", &format_args!("${:02X}", self.sp))
            .field("acc", &format_args!("${:02X}", self.acc))
            .field("x", &format_args!("${:02X}", self.x))
            .field("y", &format_args!("${:02X}", self.y))
            .field("status", &self.status)
            .field("bus", &self.bus)
            .field("instr", &self.instr)
            .field("abs_addr", &format_args!("${:04X}", self.abs_addr))
            .field("rel_addr", &format_args!("${:04X}", self.rel_addr))
            .field("fetched_data", &format_args!("${:02X}", self.fetched_data))
            .field("irq", &self.irq)
            .field("nmi", &self.nmi)
            .field("prev_nmi", &self.prev_nmi)
            .field("prev_nmi_pending", &self.prev_nmi_pending)
            .field("corrupted", &self.corrupted)
            .field("run_irq", &self.run_irq)
            .field("last_run_irq", &self.prev_run_irq)
            .field("halt", &self.halt)
            .field("dmc_dma", &self.dmc_dma)
            .field("dummy_read", &self.dummy_read)
            .field("debugging", &self.debugging)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_roms;

    #[test]
    fn cycle_timing() {
        use super::*;
        let mut cpu = Cpu::new(Bus::default());
        cpu.clock();

        assert_eq!(cpu.cycle, 15, "cpu after power + one clock");
        assert_eq!(cpu.bus.ppu.cycle_count, 44, "ppu after power + one clock");

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
            cpu.cycle = 0;
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
                cpu.cycle,
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

    test_roms!(
        "test_roms/cpu",
        branch_backward,
        nestest,
        ram_after_reset,
        regs_after_reset,
        branch_basics,
        branch_forward,
        dummy_reads,
        dummy_writes_oam,
        dummy_writes_ppumem,
        exec_space_apu,
        exec_space_ppuio,
        flag_concurrency,
        instr_abs,
        instr_abs_xy,
        instr_basics,
        instr_branches,
        instr_brk,
        instr_imm,
        instr_imp,
        instr_ind_x,
        instr_ind_y,
        instr_jmp_jsr,
        instr_misc,
        instr_rti,
        instr_rts,
        instr_special,
        instr_stack,
        instr_timing,
        instr_zp,
        instr_zp_xy,
        int_branch_delays_irq,
        int_cli_latency,
        int_irq_and_dma,
        int_nmi_and_brk,
        int_nmi_and_irq,
        overclock,
        sprdma_and_dmc_dma,
        sprdma_and_dmc_dma_512,
        timing_test,
    );
}
