//! A 6502 Central Processing Unit
//!
//! <http://wiki.nesdev.com/w/index.php/CPU>

use crate::{
    bus::Bus,
    common::{Clock, NesRegion, Regional, Reset, ResetKind},
    mem::{Access, Mem},
    ppu::{self, Ppu},
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
};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write};
use tracing::trace;

pub mod instr;

bitflags! {
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
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
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
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

/// Every cycle is either a read or a write.
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Cycle {
    start: u64,
    end: u64,
}

/// The Central Processing Unit status and registers
#[derive(Default, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Cpu {
    pub cycle: usize, // total number of cycles ran
    pub pc: u16,      // program counter
    pub dma_halt: bool,
    pub bus: Bus,
    // start/end cycle counts for reads
    pub read_cycles: Cycle,
    // start/end cycle counts for writes
    pub write_cycles: Cycle,
    pub master_clock: u64,
    pub instr: Instr,     // The currently executing instruction
    pub fetched_data: u8, // Represents data fetched for the ALU
    pub status: Status,   // Status Registers
    pub acc: u8,          // accumulator
    pub x: u8,            // x register
    pub y: u8,            // y register
    pub sp: u8,           // stack pointer - stack is at $0100-$01FF
    pub abs_addr: u16,    // Used memory addresses get set here
    pub rel_addr: u16,    // Relative address for branch instructions
    pub irq: Irq,         // Pending interrupts
    pub run_irq: bool,
    pub prev_run_irq: bool,
    pub nmi: bool,
    pub prev_nmi: bool,
    pub prev_nmi_pending: bool,
    pub dmc_dma: bool,
    #[serde(skip)]
    pub corrupted: bool, // Encountering an invalid opcode corrupts CPU processing
    pub region: NesRegion,
    pub dummy_read: bool,
    pub cycle_accurate: bool,
    #[serde(skip)]
    pub disasm: String,
}

impl Cpu {
    const NTSC_MASTER_CLOCK_RATE: f32 = 21_477_272.0;
    const NTSC_CPU_CLOCK_RATE: f32 = Self::NTSC_MASTER_CLOCK_RATE / 12.0;
    const PAL_MASTER_CLOCK_RATE: f32 = 26_601_712.0;
    const PAL_CPU_CLOCK_RATE: f32 = Self::PAL_MASTER_CLOCK_RATE / 16.0;
    const DENDY_CPU_CLOCK_RATE: f32 = Self::PAL_MASTER_CLOCK_RATE / 15.0;

    // Represents CPU/PPU alignment and would range from 0..=ppu_divider-1, if random alignment was emulated
    const PPU_OFFSET: u64 = 1;

    const NMI_VECTOR: u16 = 0xFFFA; // NMI Vector address
    const IRQ_VECTOR: u16 = 0xFFFE; // IRQ Vector address
    const RESET_VECTOR: u16 = 0xFFFC; // Vector address at reset
    const POWER_ON_STATUS: Status = Status::U.union(Status::I);
    const POWER_ON_SP: u8 = 0xFD;
    const SP_BASE: u16 = 0x0100; // Stack-pointer starting address

    pub fn new(bus: Bus) -> Self {
        let mut cpu = Self {
            cycle: 0,
            region: NesRegion::default(),
            master_clock: 0,
            read_cycles: Cycle::default(),
            write_cycles: Cycle::default(),
            pc: 0x0000,
            sp: 0x00,
            acc: 0x00,
            x: 0x00,
            y: 0x00,
            status: Self::POWER_ON_STATUS,
            bus,
            instr: Cpu::INSTRUCTIONS[0x00],
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
            dmc_dma: false,
            dma_halt: false,
            dummy_read: false,
            cycle_accurate: true,
            disasm: String::with_capacity(100),
        };
        cpu.set_region(cpu.region);
        cpu
    }

    pub fn load(&mut self, cpu: Self) {
        // Basic we don't want to serialize the entire ROM in save states, extract out the loaded
        // ROM data and merge it in with the loaded state.
        *self = Self {
            bus: Bus {
                prg_rom: std::mem::take(&mut self.bus.prg_rom),
                ppu: Ppu {
                    bus: ppu::bus::Bus {
                        chr_rom: std::mem::take(&mut self.bus.ppu.bus.chr_rom),
                        ..cpu.bus.ppu.bus
                    },
                    ..cpu.bus.ppu
                },
                ..cpu.bus
            },
            ..cpu
        };
    }

    #[must_use]
    pub const fn region_clock_rate(region: NesRegion) -> f32 {
        match region {
            NesRegion::Ntsc => Self::NTSC_CPU_CLOCK_RATE,
            NesRegion::Pal => Self::PAL_CPU_CLOCK_RATE,
            NesRegion::Dendy => Self::DENDY_CPU_CLOCK_RATE,
        }
    }

    #[must_use]
    pub const fn clock_rate(&self) -> f32 {
        Self::region_clock_rate(self.region)
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
        self.read(self.pc, Access::Dummy);
        self.read(self.pc, Access::Dummy);
        self.push_u16(self.pc);

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

            self.pc = self.read_u16(Self::NMI_VECTOR);
            trace!(
                "NMI - PPU:{:3},{:3} CYC:{}",
                self.bus.ppu.cycle(),
                self.bus.ppu.scanline(),
                self.cycle
            );
        } else {
            self.push(status);
            self.status.set(Status::I, true);

            self.pc = self.read_u16(Self::IRQ_VECTOR);
            trace!(
                "IRQ - PPU:{:3},{:3} CYC:{}",
                self.bus.ppu.cycle(),
                self.bus.ppu.scanline(),
                self.cycle
            );
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
        self.nmi |= !self.prev_nmi_pending && nmi_pending;
        self.prev_nmi_pending = nmi_pending;

        // The IRQ status at the end of the second-to-last cycle is what matters,
        // so keep the second-to-last status.
        self.prev_run_irq = self.run_irq;
        self.irq = self.bus.irqs_pending();
        self.run_irq = !self.irq.is_empty() && !self.status.intersects(Status::I);

        let dmc_dma = self.bus.apu.dmc_dma();
        self.dmc_dma |= dmc_dma;
        self.dma_halt |= dmc_dma;
        self.dummy_read |= dmc_dma;
    }

    fn start_cycle(&mut self, increment: u64) {
        self.master_clock = self.master_clock.wrapping_add(increment);
        self.cycle = self.cycle.wrapping_add(1);

        if self.cycle_accurate {
            self.bus.clock_to(self.master_clock - Self::PPU_OFFSET);
            self.bus.clock();
        }
    }

    fn end_cycle(&mut self, increment: u64) {
        self.master_clock = self.master_clock.wrapping_add(increment);

        if self.cycle_accurate {
            self.bus.clock_to(self.master_clock - Self::PPU_OFFSET);
        }

        self.handle_interrupts();
    }

    fn process_dma_cycle(&mut self) {
        // OAM DMA cycles count as halt/dummy reads for DMC DMA when both run at the same time
        if self.dma_halt {
            self.dma_halt = false;
        } else if self.dummy_read {
            self.dummy_read = false;
        }
        self.start_cycle(self.read_cycles.start);
    }

    fn handle_dma(&mut self, addr: u16) {
        self.start_cycle(self.read_cycles.start);
        self.bus.read(addr, Access::Dummy);
        self.end_cycle(self.read_cycles.end);
        self.dma_halt = false;

        let skip_dummy_reads = addr == 0x4016 || addr == 0x4017;

        let oam_base_addr = self.bus.oam_dma_addr();
        let mut oam_offset = 0;
        let mut oam_dma_count = 0;
        let mut read_val = 0;

        while self.bus.oam_dma() || self.dmc_dma {
            if self.cycle & 0x01 == 0x00 {
                if self.dmc_dma && !self.dma_halt && !self.dummy_read {
                    // DMC DMA ready to read a byte (halt and dummy read done before)
                    self.process_dma_cycle();
                    read_val = self.bus.read(self.bus.apu.dmc_dma_addr(), Access::Dummy);
                    self.end_cycle(self.read_cycles.end);
                    self.bus.apu.load_dmc_buffer(read_val);
                    self.dmc_dma = false;
                } else if self.bus.oam_dma() {
                    // DMC DMA not running or ready, run OAM DMA
                    self.process_dma_cycle();
                    read_val = self.bus.read(oam_base_addr + oam_offset, Access::Dummy);
                    self.end_cycle(self.read_cycles.end);
                    oam_offset += 1;
                    oam_dma_count += 1;
                } else {
                    // DMC DMA running, but not ready yet (needs to halt, or dummy read) and OAM
                    // DMA isn't running
                    debug_assert!(self.dma_halt || self.dummy_read);
                    self.process_dma_cycle();
                    if !skip_dummy_reads {
                        self.bus.read(addr, Access::Dummy); // throw away
                    }
                    self.end_cycle(self.read_cycles.end);
                }
            } else if self.bus.oam_dma() && oam_dma_count & 0x01 == 0x01 {
                // OAM DMA write cycle, done on odd cycles after a read on even cycles
                self.process_dma_cycle();
                self.bus.write(0x2004, read_val, Access::Dummy);
                self.end_cycle(self.read_cycles.end);
                oam_dma_count += 1;
                if oam_dma_count == 0x200 {
                    self.bus.oam_dma_finish();
                }
            } else {
                // Align to read cycle before starting OAM DMA (or align to perform DMC read)
                self.process_dma_cycle();
                if !skip_dummy_reads {
                    self.bus.read(addr, Access::Dummy); // throw away
                }
                self.end_cycle(self.read_cycles.end);
            }
        }
    }

    // Status Register functions

    // Convenience method to set both Z and N

    fn set_zn_status(&mut self, val: u8) {
        self.status.set(Status::Z, val == 0x00);
        self.status.set(Status::N, val & 0x80 == 0x80);
    }

    const fn status_bit(&self, reg: Status) -> u8 {
        self.status.intersection(reg).bits()
    }

    // Stack Functions

    // Push a byte to the stack

    fn push(&mut self, val: u8) {
        self.write(Self::SP_BASE | u16::from(self.sp), val, Access::Write);
        self.sp = self.sp.wrapping_sub(1);
    }

    // Pull a byte from the stack
    #[must_use]

    fn pop(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.read(Self::SP_BASE | u16::from(self.sp), Access::Read)
    }

    // Peek byte at the top of the stack
    #[must_use]

    pub fn peek_stack(&self) -> u8 {
        self.peek(
            Self::SP_BASE | u16::from(self.sp.wrapping_add(1)),
            Access::Dummy,
        )
    }

    // Peek at the top of the stack
    #[must_use]

    pub fn peek_stack_u16(&self) -> u16 {
        let lo = self.peek(Self::SP_BASE | u16::from(self.sp), Access::Dummy);
        let hi = self.peek(
            Self::SP_BASE | u16::from(self.sp.wrapping_add(1)),
            Access::Dummy,
        );
        u16::from_le_bytes([lo, hi])
    }

    // Push a word (two bytes) to the stack

    fn push_u16(&mut self, val: u16) {
        let [lo, hi] = val.to_le_bytes();
        self.push(hi);
        self.push(lo);
    }

    // Pull a word (two bytes) from the stack

    fn pop_u16(&mut self) -> u16 {
        let lo = self.pop();
        let hi = self.pop();
        u16::from_le_bytes([lo, hi])
    }

    // Memory accesses

    // Source the data used by an instruction. Some instructions don't fetch data as the source
    // is implied by the instruction such as INX which increments the X register.
    fn fetch_data(&mut self) {
        let mode = self.instr.addr_mode();
        let acc = self.acc;
        let abs_addr = self.abs_addr;
        self.fetched_data = if matches!(mode, IMP | ACC) {
            acc
        } else {
            self.read(abs_addr, Access::Read) // Cycle 2/4/5 read
        };
    }

    // Read instructions may have crossed a page boundary and need to be re-read
    fn fetch_data_cross(&mut self) {
        let mode = self.instr.addr_mode();
        let x = self.x;
        let y = self.y;
        let abs_addr = self.abs_addr;
        if matches!(mode, ABX | ABY | IDY) {
            let reg = match mode {
                ABX => x,
                ABY | IDY => y,
                _ => unreachable!("not possible"),
            };
            // Read if we crossed, otherwise use what was already set in cycle 4 from
            // addressing mode
            //
            // ABX/ABY/IDY all add `reg` to `abs_addr`, so this checks if it wrapped
            // around to 0.
            if (abs_addr & 0x00FF) < u16::from(reg) {
                self.fetched_data = self.read(abs_addr, Access::Read);
            }
        } else {
            self.fetch_data();
        }
    }

    // Writes data back to where fetched_data was sourced from. Either accumulator or memory
    // specified in abs_addr.

    fn write_fetched(&mut self, val: u8) {
        match self.instr.addr_mode() {
            IMP | ACC => self.acc = val,
            IMM => (), // noop
            _ => self.write(self.abs_addr, val, Access::Write),
        }
    }

    // Reads an instruction byte and increments PC by 1.
    #[must_use]

    fn read_instr(&mut self) -> u8 {
        let val = self.read(self.pc, Access::Read);
        self.pc = self.pc.wrapping_add(1);
        val
    }

    // Reads an instruction 16-bit word and increments PC by 2.
    #[must_use]

    fn read_instr_u16(&mut self) -> u16 {
        let lo = self.read_instr();
        let hi = self.read_instr();
        u16::from_le_bytes([lo, hi])
    }

    // Read a 16-bit word.
    #[must_use]

    pub fn read_u16(&mut self, addr: u16) -> u16 {
        let lo = self.read(addr, Access::Read);
        let hi = self.read(addr.wrapping_add(1), Access::Read);
        u16::from_le_bytes([lo, hi])
    }

    // Peek a 16-bit word without side effects.
    #[must_use]

    pub fn peek_u16(&self, addr: u16) -> u16 {
        let lo = self.peek(addr, Access::Dummy);
        let hi = self.peek(addr.wrapping_add(1), Access::Dummy);
        u16::from_le_bytes([lo, hi])
    }

    // Like read_word, but for Zero Page which means it'll wrap around at 0xFF
    #[must_use]

    fn read_zp_u16(&mut self, addr: u8) -> u16 {
        let lo = self.read(addr.into(), Access::Read);
        let hi = self.read(addr.wrapping_add(1).into(), Access::Read);
        u16::from_le_bytes([lo, hi])
    }

    // Like peek_word, but for Zero Page which means it'll wrap around at 0xFF
    #[must_use]

    fn peek_zp_u16(&self, addr: u8) -> u16 {
        let lo = self.peek(addr.into(), Access::Dummy);
        let hi = self.peek(addr.wrapping_add(1).into(), Access::Dummy);
        u16::from_le_bytes([lo, hi])
    }

    pub fn disassemble(&mut self, pc: &mut u16) -> &str {
        let opcode = self.peek(*pc, Access::Dummy);
        let instr = Cpu::INSTRUCTIONS[opcode as usize];
        self.disasm.clear();

        let _ = write!(self.disasm, "${pc:04X} ${opcode:02X} ");
        let mut addr = pc.wrapping_add(1);

        match instr.addr_mode() {
            IMM => {
                let byte = self.peek(addr, Access::Dummy);
                addr = addr.wrapping_add(1);
                let _ = write!(self.disasm, "${byte:02X}     {instr} #${byte:02X}");
            }
            ZP0 => {
                let byte = self.peek(addr, Access::Dummy);
                addr = addr.wrapping_add(1);
                let val = self.peek(byte.into(), Access::Dummy);
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr} ${byte:02X} = #${val:02X}"
                );
            }
            ZPX => {
                let byte = self.peek(addr, Access::Dummy);
                addr = addr.wrapping_add(1);
                let x_offset = byte.wrapping_add(self.x);
                let val = self.peek(x_offset.into(), Access::Dummy);
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr} ${byte:02X},X @ ${x_offset:02X} = #${val:02X}"
                );
            }
            ZPY => {
                let byte = self.peek(addr, Access::Dummy);
                addr = addr.wrapping_add(1);
                let y_offset = byte.wrapping_add(self.y);
                let val = self.peek(y_offset.into(), Access::Dummy);
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr} ${byte:02X},Y @ ${y_offset:02X} = #${val:02X}"
                );
            }
            ABS => {
                let byte1 = self.peek(addr, Access::Dummy);
                let byte2 = self.peek(addr.wrapping_add(1), Access::Dummy);
                let abs_addr = self.peek_u16(addr);
                addr = addr.wrapping_add(2);
                if instr.op() == JMP || instr.op() == JSR {
                    let _ = write!(
                        self.disasm,
                        "${byte1:02X} ${byte2:02X} {instr} ${abs_addr:04X}"
                    );
                } else {
                    let val = self.peek(abs_addr, Access::Dummy);
                    let _ = write!(
                        self.disasm,
                        "${byte1:02X} ${byte2:02X} {instr} ${abs_addr:04X} = #${val:02X}"
                    );
                }
            }
            ABX => {
                let byte1 = self.peek(addr, Access::Dummy);
                let byte2 = self.peek(addr.wrapping_add(1), Access::Dummy);
                let abs_addr = self.peek_u16(addr);
                addr = addr.wrapping_add(2);
                let x_offset = abs_addr.wrapping_add(self.x.into());
                let val = self.peek(x_offset, Access::Dummy);
                let _ = write!(self.disasm, "${byte1:02X} ${byte2:02X} {instr} ${abs_addr:04X},X @ ${x_offset:04X} = #${val:02X}");
            }
            ABY => {
                let byte1 = self.peek(addr, Access::Dummy);
                let byte2 = self.peek(addr.wrapping_add(1), Access::Dummy);
                let abs_addr = self.peek_u16(addr);
                addr = addr.wrapping_add(2);
                let y_offset = abs_addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset, Access::Dummy);
                let _ = write!(self.disasm, "${byte1:02X} ${byte2:02X} {instr} ${abs_addr:04X},Y @ ${y_offset:04X} = #${val:02X}");
            }
            IND => {
                let byte1 = self.peek(addr, Access::Dummy);
                let byte2 = self.peek(addr.wrapping_add(1), Access::Dummy);
                let abs_addr = self.peek_u16(addr);
                addr = addr.wrapping_add(2);
                let lo = self.peek(abs_addr, Access::Dummy);
                let hi = if abs_addr & 0x00FF == 0x00FF {
                    self.peek(abs_addr & 0xFF00, Access::Dummy)
                } else {
                    self.peek(abs_addr + 1, Access::Dummy)
                };
                let val = u16::from_le_bytes([lo, hi]);
                let _ = write!(
                    self.disasm,
                    "${byte1:02X} ${byte2:02X} {instr} (${abs_addr:04X}) = ${val:04X}"
                );
            }
            IDX => {
                let byte = self.peek(addr, Access::Dummy);
                addr = addr.wrapping_add(1);
                let x_offset = byte.wrapping_add(self.x);
                let abs_addr = self.peek_zp_u16(x_offset);
                let val = self.peek(abs_addr, Access::Dummy);
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr} (${byte:02X},X) @ ${abs_addr:04X} = #${val:02X}"
                );
            }
            IDY => {
                let byte = self.peek(addr, Access::Dummy);
                addr = addr.wrapping_add(1);
                let abs_addr = self.peek_zp_u16(byte);
                let y_offset = abs_addr.wrapping_add(self.y.into());
                let val = self.peek(y_offset, Access::Dummy);
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr} (${byte:02X}),Y @ ${y_offset:04X} = #${val:02X}"
                );
            }
            REL => {
                let byte = self.peek(addr, Access::Dummy);
                let mut rel_addr = self.peek(addr, Access::Dummy).into();
                addr = addr.wrapping_add(1);
                if rel_addr & 0x80 == 0x80 {
                    // If address is negative, extend sign to 16-bits
                    rel_addr |= 0xFF00;
                }
                rel_addr = addr.wrapping_add(rel_addr);
                let _ = write!(self.disasm, "${byte:02X}     {instr} ${rel_addr:04X}");
            }
            ACC | IMP => (),
        };
        *pc = addr;
        &self.disasm
    }

    // Return the current instruction and status
    pub fn trace_instr(&mut self) {
        let mut pc = self.pc;
        let status = self.status;
        let acc = self.acc;
        let x = self.x;
        let y = self.y;
        let sp = self.sp;
        let ppu_cycle = self.bus.ppu.cycle();
        let ppu_scanline = self.bus.ppu.scanline();
        let cycle = self.cycle;
        let n = if status.contains(Status::N) { 'N' } else { 'n' };
        let v = if status.contains(Status::V) { 'V' } else { 'v' };
        let i = if status.contains(Status::I) { 'I' } else { 'i' };
        let z = if status.contains(Status::Z) { 'Z' } else { 'z' };
        let c = if status.contains(Status::C) { 'C' } else { 'c' };
        trace!(
            "{:<50} A:{acc:02X} X:{x:02X} Y:{y:02X} P:{n}{v}--d{i}{z}{c} SP:{sp:02X} PPU:{ppu_cycle:3},{ppu_scanline:3} CYC:{cycle}",
            self.disassemble(&mut pc),
        );
    }

    /// Utilities

    #[must_use]

    const fn pages_differ(addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }
}

impl Clock for Cpu {
    /// Runs the CPU one instruction
    fn clock(&mut self) -> usize {
        let start_cycle = self.cycle;

        self.trace_instr();

        let opcode = self.read_instr(); // Cycle 1 of instruction
        self.instr = Cpu::INSTRUCTIONS[opcode as usize];

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

        if !self.cycle_accurate {
            self.bus.clock_to(self.master_clock - Self::PPU_OFFSET);
            let cycles = self.cycle - start_cycle;
            for _ in 0..cycles {
                self.bus.clock();
            }
            self.handle_interrupts();
        }

        self.cycle - start_cycle
    }
}

impl Mem for Cpu {
    fn read(&mut self, addr: u16, access: Access) -> u8 {
        if self.dma_halt || self.bus.oam_dma() {
            self.handle_dma(addr);
        }

        self.start_cycle(self.read_cycles.start);
        let val = self.bus.read(addr, access);
        self.end_cycle(self.read_cycles.end);
        val
    }

    fn peek(&self, addr: u16, access: Access) -> u8 {
        self.bus.peek(addr, access)
    }

    fn write(&mut self, addr: u16, val: u8, access: Access) {
        self.start_cycle(self.write_cycles.start);
        self.bus.write(addr, val, access);
        self.end_cycle(self.write_cycles.end);
    }
}

impl Regional for Cpu {
    fn region(&self) -> NesRegion {
        self.region
    }

    fn set_region(&mut self, region: NesRegion) {
        let (start_clocks, end_clocks) = match region {
            NesRegion::Ntsc => (6, 6),
            NesRegion::Pal => (8, 8),
            NesRegion::Dendy => (7, 8),
        };
        self.region = region;
        self.read_cycles = Cycle {
            start: start_clocks - 1,
            end: end_clocks + 1,
        };
        self.write_cycles = Cycle {
            start: end_clocks + 1,
            end: end_clocks - 1,
        };
        self.bus.set_region(region);
    }
}

impl Reset for Cpu {
    /// Resets the CPU
    ///
    /// Updates the PC, SP, and Status values to defined constants.
    ///
    /// These operations take the CPU 7 cycles.
    fn reset(&mut self, kind: ResetKind) {
        trace!("{:?} RESET", kind);

        match kind {
            ResetKind::Soft => {
                self.status.set(Status::I, true);
                // Reset pushes to the stack similar to IRQ, but since the read bit is set, nothing is
                // written except the SP being decremented
                self.sp = self.sp.wrapping_sub(0x03);
            }
            ResetKind::Hard => {
                self.acc = 0x00;
                self.x = 0x00;
                self.y = 0x00;
                self.status = Self::POWER_ON_STATUS;
                self.sp = Self::POWER_ON_SP;
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
        self.dma_halt = false;
        self.dummy_read = false;

        // Read directly from bus so as to not clock other components during reset
        let lo = self.bus.read(Self::RESET_VECTOR, Access::Read);
        let hi = self.bus.read(Self::RESET_VECTOR + 1, Access::Read);
        self.pc = u16::from_le_bytes([lo, hi]);

        for _ in 0..7 {
            self.start_cycle(self.read_cycles.start);
            self.end_cycle(self.read_cycles.end);
        }
    }
}

impl fmt::Debug for Cpu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Cpu")
            .field("cycle", &self.cycle)
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
            .field("halt", &self.dma_halt)
            .field("dummy_read", &self.dummy_read)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::cart::Cart;

    #[test]
    fn cycle_timing() {
        use super::*;
        let mut cpu = Cpu::new(Bus::default());
        let cart = Cart::empty();
        cpu.bus.load_cart(cart);
        cpu.reset(ResetKind::Hard);
        cpu.clock();

        assert_eq!(cpu.cycle, 14, "cpu after power + one clock");

        for instr in Cpu::INSTRUCTIONS.iter() {
            let extra_cycle = match instr.op() {
                BCC | BNE | BPL | BVC => 1,
                _ => 0,
            };
            // Ignore invalid opcodes
            if instr.op() == XXX {
                continue;
            }
            cpu.reset(ResetKind::Hard);
            cpu.bus.write(0x0000, instr.opcode(), Access::Write);
            cpu.clock();
            let cpu_cyc = 7 + instr.cycles() + extra_cycle;
            assert_eq!(
                cpu.cycle,
                cpu_cyc,
                "cpu ${:02X} {:?} #{:?}",
                instr.opcode(),
                instr.op(),
                instr.addr_mode()
            );
        }
    }
}
