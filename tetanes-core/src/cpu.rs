//! 6502 Central Processing Unit (CPU) implementation.
//!
//! <https://wiki.nesdev.org/w/index.php/CPU>

use crate::cpu::instr::{
    AddrMode,
    Instr::{JMP, JSR},
    InstrRef,
};
use crate::{
    bus::Bus,
    common::{Clock, ClockTo, NesRegion, Regional, Reset, ResetKind},
    mem::{Read, Write},
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::fmt::{self};
use tracing::trace;

pub mod instr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInterrupts {
    nmi: bool,
    irqs: Irq,
    dmas: Dma,
    dma_halt: bool,
    dma_dummy_read: bool,
    dma_oam_addr: u16,
}

impl Default for CpuInterrupts {
    fn default() -> Self {
        Self {
            nmi: false,
            irqs: Irq::empty(),
            dmas: Dma::empty(),
            dma_halt: false,
            dma_dummy_read: false,
            dma_oam_addr: 0,
        }
    }
}

impl CpuInterrupts {
        #[inline]
    #[must_use]
    pub fn nmi_pending(&self) -> bool {
        self.nmi
    }

    #[inline]
    pub fn set_nmi(&mut self) {
        self.nmi = true;
    }

    #[inline]
    pub fn clear_nmi(&mut self) {
        self.nmi = false;
    }

    #[inline]
    pub fn irqs(&self) -> Irq {
        self.irqs
    }

    #[inline]
    #[must_use]
    pub fn has_irq(&self, irq: Irq) -> bool {
        self.irqs.contains(irq)
    }

    #[inline]
    pub fn set_irq(&mut self, irq: Irq) {
        self.irqs |= irq;
    }

    #[inline]
    pub fn clear_irq(&mut self, irq: Irq) {
        self.irqs = self.irqs & !irq;
    }

    #[inline]
    pub fn start_dmc_dma(&mut self) {
        self.dmas |= Dma::DMC;
        self.dma_halt = true;
        self.dma_dummy_read = true;
    }

    #[inline]
    pub fn start_oam_dma(&mut self, addr: u16) {
        self.dmas |= Dma::OAM;
        self.dma_halt = true;
        self.dma_oam_addr = addr;
    }

    #[inline]
    #[must_use]
    pub fn halt_for_dma(&self) -> bool {
        self.dma_halt
    }

    #[inline]
    pub fn dma_oam_addr(&self) -> u16 {
        self.dma_oam_addr
    }

    #[inline]
    #[must_use]
    pub fn dmas_running(&self) -> Option<(bool, bool)> {
        let dmas = self.dmas;
        (!dmas.is_empty()).then_some((dmas.contains(Dma::DMC), dmas.contains(Dma::OAM)))
    }

    #[inline]
    pub fn clear_dma(&mut self, dma: Dma) {
        self.dmas = self.dmas & !dma;
    }

    #[inline]
    pub fn clear_dma_halt(&mut self) {
        self.dma_halt = false;
    }

    #[inline]
    pub fn dma_dummy_read(&self) -> bool {
        self.dma_dummy_read
    }

    #[inline]
    pub fn clear_dma_dummy_read(&mut self) {
        self.dma_dummy_read = false;
    }
}

bitflags! {
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
    #[must_use]
    pub struct Irq: u8 {
        const MAPPER = 1 << 0;
        const FRAME_COUNTER = 1 << 1;
        const DMC = 1 << 2;
    }
}

bitflags! {
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
    #[must_use]
    pub struct Dma: u8 {
        const OAM = 1 << 0;
        const DMC = 1 << 1;
    }
}

bitflags! {
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
    #[must_use]
    pub struct IrqFlags: u16 {
        const NMI = 1 << 0;
        const PREV_NMI = 1 << 1;
        const PREV_NMI_PENDING = 1 << 2;
        const RUN_IRQ = 1 << 3;
        const PREV_RUN_IRQ = 1 << 4;
    }
}

// Status Registers
// https://wiki.nesdev.org/w/index.php/Status_flags
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
    /// CPU Status Registers.
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

/// The Central Processing Unit status and registers
#[derive(Default, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(C)]
pub struct Cpu {
    pub cycle: u32, // total number of cycles ran
    pub master_clock: u32,
    // start/end cycle counts for reads/writes
    pub start_cycles: u8,
    pub end_cycles: u8,
    pub pc: u16,             // program counter
    pub operand: u16,        // opcode operand
    pub addr_mode: AddrMode, // Addressing mode
    pub sp: u8,              // stack pointer - stack is at $0100-$01FF
    pub acc: u8,             // accumulator
    pub x: u8,               // x register
    pub y: u8,               // y register
    pub status: Status,      // Status Registers
    pub irq_flags: IrqFlags,
    pub bus: Bus,
    #[serde(skip)]
    pub corrupted: bool, // Encountering an invalid opcode corrupts CPU processing
    #[serde(skip)]
    pub disasm: String
}

impl Cpu {
    const NTSC_MASTER_CLOCK_RATE: f32 = 21_477_272.0;
    const NTSC_CPU_CLOCK_RATE: f32 = Self::NTSC_MASTER_CLOCK_RATE / 12.0;
    const PAL_MASTER_CLOCK_RATE: f32 = 26_601_712.0;
    const PAL_CPU_CLOCK_RATE: f32 = Self::PAL_MASTER_CLOCK_RATE / 16.0;
    const DENDY_CPU_CLOCK_RATE: f32 = Self::PAL_MASTER_CLOCK_RATE / 15.0;

    // Represents CPU/PPU alignment and would range from 1..=ppu.clock_divider-1
    // if random PPU alignment was emulated
    // See: https://www.nesdev.org/wiki/PPU_frame_timing#CPU-PPU_Clock_Alignment
    const PPU_OFFSET: u32 = 1;

    const NMI_VECTOR: u16 = 0xFFFA; // NMI Vector address
    const IRQ_VECTOR: u16 = 0xFFFE; // IRQ Vector address
    const RESET_VECTOR: u16 = 0xFFFC; // Vector address at reset
    const POWER_ON_STATUS: Status = Status::U.union(Status::I);
    const POWER_ON_SP: u8 = 0xFD;
    const SP_BASE: u16 = 0x0100; // Stack-pointer starting address

    /// Create a new CPU with the given bus.
    pub fn new(bus: Bus) -> Self {
        let mut cpu = Self {
            cycle: 0,
            master_clock: 0,
            start_cycles: 6,
            end_cycles: 6,
            pc: 0x0000,
            operand: 0,
            addr_mode: AddrMode::default(),
            sp: 0x00,
            acc: 0x00,
            x: 0x00,
            y: 0x00,
            status: Self::POWER_ON_STATUS,
            irq_flags: IrqFlags::default(),
            bus,
            corrupted: false,
            disasm: String::new()
        };
        let mut intrs = CpuInterrupts::default();
        cpu.set_region(cpu.bus.region, &mut intrs);
        cpu
    }

    /// Load a CPU state.
    pub fn load(&mut self, mut cpu: Self) {
        // Because we don't want to serialize the entire ROM in save states, extract out the
        // already loaded ROM data if it's not provided
        if cpu.bus.prg_rom.is_empty() {
            cpu.bus.prg_rom = std::mem::take(&mut self.bus.prg_rom);
        };
        if cpu.bus.ppu.bus.chr.is_empty() {
            cpu.bus.ppu.bus.chr = std::mem::take(&mut self.bus.ppu.bus.chr);
        };
        // Doesn't make sense to load a debugger from a previous state
        cpu.bus.ppu.debugger = std::mem::take(&mut self.bus.ppu.debugger);
        *self = cpu;
    }

    /// Returns the CPU clock rate based on [`NesRegion`].
    #[inline]
    #[must_use]
    pub const fn region_clock_rate(region: NesRegion) -> f32 {
        match region {
            NesRegion::Auto | NesRegion::Ntsc => Self::NTSC_CPU_CLOCK_RATE,
            NesRegion::Pal => Self::PAL_CPU_CLOCK_RATE,
            NesRegion::Dendy => Self::DENDY_CPU_CLOCK_RATE,
        }
    }

    /// Clock rate based on currently configured NES region.
    #[inline]
    #[must_use]
    pub const fn clock_rate(&self) -> f32 {
        Self::region_clock_rate(self.bus.region)
    }

    /// Peek at the next instruction.
    #[inline]
    pub fn next_instr(&self) -> InstrRef {
        let opcode = self.peek(self.pc);
        Cpu::INSTR_REF[usize::from(opcode)]
    }

    /// Process an interrupted request.
    ///
    /// <https://wiki.nesdev.org/w/index.php/IRQ>
    ///  #  address R/W description
    /// --- ------- --- -----------------------------------------------
    ///  1    PC     R  fetch PCH
    ///  2    PC     R  fetch PCL
    ///  3  $0100,S  W  push PCH to stack, decrement S
    ///  4  $0100,S  W  push PCL to stack, decrement S
    ///  5  $0100,S  W  push P to stack, decrement S
    ///  6    PC     R  fetch low byte of interrupt vector
    ///  7    PC     R  fetch high byte of interrupt vector
    #[cold]
    #[inline(never)]
    pub fn irq(&mut self, intrs: &mut CpuInterrupts) {
        if intrs.halt_for_dma() && self.region() == NesRegion::Pal {
            // Check for DMA on PAL
            self.handle_dma(self.pc, intrs);
        }

        self.read(self.pc, intrs); // Dummy read
        self.read(self.pc, intrs); // Dummy read
        self.push_word(self.pc, intrs);

        // Pushing status to the stack has to happen after checking NMI since it can hijack the BRK
        // IRQ when it occurs between cycles 4 and 5.
        // https://www.nesdev.org/wiki/CPU_interrupts#Interrupt_hijacking
        //
        // Set U and !B during push
        let status = ((self.status | Status::U) & !Status::B).bits();
        let nmi = self.irq_flags(IrqFlags::NMI);
        self.push_byte(status, intrs);
        self.status.set(Status::I, true);

        if nmi {
            self.clear_irq_flags(IrqFlags::NMI);
            self.pc = self.read_word(Self::NMI_VECTOR, intrs);
            self.bus.ppu.clock_to(self.master_clock, intrs);
            self.master_clock = self.master_clock.saturating_sub(self.bus.ppu.master_clock);
            self.bus.ppu.master_clock = 0;
            trace!(
                "NMI - PPU:{:3},{:3} CYC:{}",
                self.bus.ppu.cycle, self.bus.ppu.scanline, self.cycle
            );
        } else {
            self.pc = self.read_word(Self::IRQ_VECTOR, intrs);
            trace!(
                "IRQ - PPU:{:3},{:3} CYC:{}",
                self.bus.ppu.cycle, self.bus.ppu.scanline, self.cycle
            );
        }
    }

    /// Handle CPU interrupt requests, if any are pending.
    #[inline(always)]
    fn handle_interrupts(&mut self, intrs: &mut CpuInterrupts) {
        // https://www.nesdev.org/wiki/CPU_interrupts
        //
        // The internal signal goes high during φ1 of the cycle that follows the one where
        // the edge is detected, and stays high until the NMI has been handled. NMI is handled only
        // when `prev_nmi` is true.
        self.irq_flags
            .set(IrqFlags::PREV_NMI, self.irq_flags.contains(IrqFlags::NMI));

        // This edge detector polls the status of the NMI line during φ2 of each CPU cycle (i.e.,
        // during the second half of each cycle, hence here in `end_cycle`) and raises an internal
        // signal if the input goes from being high during one cycle to being low during the
        // next.
        let nmi_pending = intrs.nmi_pending();
        let prev_nmi_pending = self.irq_flags.contains(IrqFlags::PREV_NMI_PENDING);
        if !prev_nmi_pending && nmi_pending {
            self.irq_flags.insert(IrqFlags::NMI);
        }
        self.irq_flags.set(IrqFlags::PREV_NMI_PENDING, nmi_pending);

        // The IRQ status at the end of the second-to-last cycle is what matters,
        // so keep the second-to-last status.
        self.irq_flags.set(
            IrqFlags::PREV_RUN_IRQ,
            self.irq_flags.contains(IrqFlags::RUN_IRQ),
        );
        let irqs = intrs.irqs;
        let run_irq = !irqs.is_empty() && !self.status.intersects(Status::I);
        self.irq_flags.set(IrqFlags::RUN_IRQ, run_irq);

        #[cfg(feature = "trace")]
        if !flags.contains(IrqFlags::PREV_NMI_PENDING) && flags.contains(IrqFlags::RUN_IRQ) {
            trace!("IRQs: {:?} - CYC:{}", irqs, self.cycle);
        }
    }

    /// Start a CPU cycle.
    #[inline(always)]
    fn start_cycle(&mut self, increment: u8, intrs: &mut CpuInterrupts) {
        self.master_clock += u32::from(increment);
        self.cycle = self.cycle.wrapping_add(1);
        self.bus.clock_to(self.master_clock - Self::PPU_OFFSET, intrs);
        self.bus.clock(intrs);
    }

    /// End a CPU cycle.
    #[inline(always)]
    fn end_cycle(&mut self, increment: u8, intrs: &mut CpuInterrupts) {
        self.master_clock += u32::from(increment);
        self.bus.clock_to(self.master_clock - Self::PPU_OFFSET, intrs);

        self.handle_interrupts(intrs);
    }

    /// Start a direct-memory access (DMA) cycle.
    #[inline(always)]
    fn start_dma_cycle(&mut self, intrs: &mut CpuInterrupts) {
        // OAM DMA cycles count as halt/dummy reads for DMC DMA when both run at the same time
        if intrs.halt_for_dma() {
            intrs.clear_dma_halt();
        } else {
            intrs.clear_dma_dummy_read();
        }
        self.start_cycle(self.start_cycles - 1, intrs);
    }

    /// Handle a direct-memory access (DMA) request.
    #[cold]
    #[inline(never)]
    fn handle_dma(&mut self, addr: u16, intrs: &mut CpuInterrupts) {
        trace!("Starting DMA - CYC:{}", self.cycle);

        self.start_cycle(self.start_cycles - 1, intrs);
        self.bus.read(addr, intrs);
        self.end_cycle(self.start_cycles + 1, intrs);
        intrs.clear_dma_halt();

        let skip_dummy_reads = addr == 0x4016 || addr == 0x4017;

        let mut oam_offset = 0;
        let mut oam_dma_count = 0;
        let mut read_val = 0;

        while let Some((dmc_dma, oam_dma)) = intrs.dmas_running() {
            if self.cycle & 0x01 == 0x00 {
                if dmc_dma && !intrs.halt_for_dma() && !intrs.dma_dummy_read() {
                    // DMC DMA ready to read a byte (halt and dummy read done before)
                    self.start_dma_cycle(intrs);
                    let dma_addr = self.bus.apu.dmc.dma_addr();
                    read_val = self.bus.read(dma_addr, intrs);
                    trace!(
                        "Loaded DMC DMA byte. ${dma_addr:04X}: {read_val} - CYC:{}",
                        self.cycle
                    );
                    self.end_cycle(self.start_cycles + 1, intrs);
                    self.bus.apu.dmc.load_buffer(read_val, intrs);
                    intrs.clear_dma(Dma::DMC);
                } else if oam_dma {
                    // DMC DMA not running or ready, run OAM DMA
                    self.start_dma_cycle(intrs);
                    read_val = self.bus.read(intrs.dma_oam_addr() + oam_offset, intrs);
                    self.end_cycle(self.start_cycles + 1, intrs);
                    oam_offset += 1;
                    oam_dma_count += 1;
                } else {
                    // DMC DMA running, but not ready yet (needs to halt, or dummy read) and OAM
                    // DMA isn't running
                    debug_assert!(intrs.halt_for_dma() || intrs.dma_dummy_read());
                    self.start_dma_cycle(intrs);
                    if !skip_dummy_reads {
                        self.bus.read(addr, intrs); // throw away
                    }
                    self.end_cycle(self.start_cycles + 1, intrs);
                }
            } else if oam_dma && oam_dma_count & 0x01 == 0x01 {
                // OAM DMA write cycle, done on odd cycles after a read on even cycles
                self.start_dma_cycle(intrs);
                self.bus.write(0x2004, read_val, intrs);
                self.end_cycle(self.start_cycles + 1, intrs);
                oam_dma_count += 1;
                if oam_dma_count == 0x200 {
                    intrs.clear_dma(Dma::OAM);
                }
            } else {
                // Align to read cycle before starting OAM DMA (or align to perform DMC read)
                self.start_dma_cycle(intrs);
                if !skip_dummy_reads {
                    self.bus.read(addr, intrs); // throw away
                }
                self.end_cycle(self.start_cycles + 1, intrs);
            }
        }
    }

    // Interrupt flag functions

    /// Clear [`IrqFlags`] flags for the given bits.
    #[inline(always)]
    fn clear_irq_flags(&mut self, flags: IrqFlags) {
        self.irq_flags &= !flags;
    }

    /// Returns `true` if the [`IrqFlags`] register is set.
    #[inline(always)]
    fn irq_flags(&self, flags: IrqFlags) -> bool {
        (self.irq_flags & flags).bits() == flags.bits()
    }

    // Status Register functions

    /// Set [`Status`] flags for the given bits.
    #[inline(always)]
    fn set_status(&mut self, status: Status) {
        self.status = status & !Status::U & !Status::B;
    }

    /// Returns the [`Status`] register as a byte.
    #[inline(always)]
    const fn status_bit(&self, reg: Status) -> u8 {
        self.status.intersection(reg).bits()
    }

    /// Set accumulator and update [`Status`] flags based on value.
    #[inline(always)]
    fn set_acc(&mut self, val: u8) {
        self.set_zn_status(val);
        self.acc = val;
    }

    /// Set x and update [`Status`] flags based on value.
    #[inline(always)]
    fn set_x(&mut self, val: u8) {
        self.set_zn_status(val);
        self.x = val;
    }

    /// Set y and update [`Status`] flags based on value.
    #[inline(always)]
    fn set_y(&mut self, val: u8) {
        self.set_zn_status(val);
        self.y = val;
    }

    /// Set stack pointer.
    #[inline(always)]
    const fn set_sp(&mut self, val: u8) {
        self.sp = val;
    }

    /// Set both [`Status::Z`] and [`Status::N`] flags based on value.
    #[inline(always)]
    fn set_zn_status(&mut self, val: u8) {
        self.status.set(Status::Z, val == 0x00);
        self.status.set(Status::N, val & 0x80 > 0);
    }

    // Stack Functions

    /// Push a byte to the stack.
    #[inline(always)]
    fn push_byte(&mut self, val: u8, intrs: &mut CpuInterrupts) {
        self.write(Self::SP_BASE | u16::from(self.sp), val, intrs);
        self.sp = self.sp.wrapping_sub(1);
    }

    /// Pull a byte from the stack.
    #[inline(always)]
    #[must_use]
    fn pop_byte(&mut self, intrs: &mut CpuInterrupts) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.read(Self::SP_BASE | u16::from(self.sp), intrs)
    }

    /// Peek byte at the top of the stack.
    #[inline]
    #[must_use]
    pub fn peek_stack(&self) -> u8 {
        self.peek(Self::SP_BASE | u16::from(self.sp.wrapping_add(1)))
    }

    /// Peek at the top of the stack.
    #[inline]
    #[must_use]
    pub fn peek_stack_u16(&self) -> u16 {
        let lo = self.peek(Self::SP_BASE | u16::from(self.sp));
        let hi = self.peek(Self::SP_BASE | u16::from(self.sp.wrapping_add(1)));
        u16::from_le_bytes([lo, hi])
    }

    /// Push a word (two bytes) to the stack
    #[inline(always)]
    fn push_word(&mut self, val: u16, intrs: &mut CpuInterrupts) {
        let [lo, hi] = val.to_le_bytes();
        self.push_byte(hi, intrs);
        self.push_byte(lo, intrs);
    }

    /// Pull a word (two bytes) from the stack
    #[inline(always)]
    fn pop_word(&mut self, intrs: &mut CpuInterrupts) -> u16 {
        let lo = self.pop_byte(intrs);
        let hi = self.pop_byte(intrs);
        u16::from_le_bytes([lo, hi])
    }

    // Memory accesses

    /// Fetch a byte and increments PC by 1.
    #[inline(always)]
    #[must_use]
    fn fetch_byte(&mut self, intrs: &mut CpuInterrupts) -> u8 {
        let val = self.read(self.pc, intrs);
        self.pc = self.pc.wrapping_add(1);
        val
    }

    /// Fetch opcode operand based on addressing mode.
    #[inline(always)]
    #[must_use]
    fn fetch_operand(&mut self, intrs: &mut CpuInterrupts) -> u16 {
        match self.addr_mode {
            AddrMode::ACC | AddrMode::IMP => self.acc_imp(intrs),
            AddrMode::IMM | AddrMode::REL | AddrMode::ZP0 => self.imm_rel_zp(intrs),
            AddrMode::ZPX => self.zpx(intrs),
            AddrMode::ZPY => self.zpy(intrs),
            AddrMode::IND => self.ind(intrs),
            AddrMode::IDX => self.idx(intrs),
            AddrMode::IDY => self.idy(false, intrs),
            AddrMode::IDYW => self.idy(true, intrs),
            AddrMode::ABS => self.abs(intrs),
            AddrMode::ABX => self.abx(false, intrs),
            AddrMode::ABXW => self.abx(true, intrs),
            AddrMode::ABY => self.aby(false, intrs),
            AddrMode::ABYW => self.aby(true, intrs),
            AddrMode::OTH => 0,
        }
    }

    /// Fetch a 16-bit word and increments PC by 2.
    #[inline(always)]
    #[must_use]
    fn fetch_word(&mut self, intrs: &mut CpuInterrupts) -> u16 {
        let lo = self.fetch_byte(intrs);
        let hi = self.fetch_byte(intrs);
        u16::from_le_bytes([lo, hi])
    }

    /// Read operand value.
    #[inline(always)]
    #[must_use]
    fn read_operand(&mut self, intrs: &mut CpuInterrupts) -> u8 {
        if matches!(
            self.addr_mode,
            AddrMode::ACC | AddrMode::IMP | AddrMode::IMM | AddrMode::REL
        ) {
            self.operand as u8
        } else {
            self.read(self.operand, intrs)
        }
    }

    /// Read a 16-bit word.
    #[inline(always)]
    #[must_use]
    pub fn read_word(&mut self, addr: u16, intrs: &mut CpuInterrupts) -> u16 {
        let lo = self.read(addr, intrs);
        let hi = self.read(addr.wrapping_add(1), intrs);
        u16::from_le_bytes([lo, hi])
    }

    /// Peek a 16-bit word without side effects.
    #[inline]
    #[must_use]
    pub fn peek_word(&self, addr: u16) -> u16 {
        let lo = self.peek(addr);
        let hi = self.peek(addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    /// Disassemble the instruction at the given program counter.
    pub fn disassemble(&mut self, pc: &mut u16) -> &str {
        use fmt::Write;

        self.disasm.clear();

        let addr = { *pc };
        let opcode = {
            let byte = self.peek(*pc);
            *pc = pc.wrapping_add(1);
            byte
        };
        let _ = write!(self.disasm, "${addr:04X} ${opcode:02X} ");

        let mut peek_byte = || {
            let byte = self.peek(*pc);
            *pc = pc.wrapping_add(1);
            byte
        };
        let mut peek_word = || {
            let lo = peek_byte();
            let hi = peek_byte();
            (lo, hi, u16::from_le_bytes([lo, hi]))
        };

        let instr_ref = Cpu::INSTR_REF[usize::from(opcode)];
        match instr_ref.addr_mode {
            AddrMode::ACC | AddrMode::IMP => {
                let _ = write!(self.disasm, "        {instr_ref}");
            }
            AddrMode::IMM => {
                let byte = peek_byte();
                let _ = write!(self.disasm, "${byte:02X}     {instr_ref} #${byte:02X}");
            }
            AddrMode::REL => {
                let byte = peek_byte();
                let addr = (*pc as i16).wrapping_add(i16::from(byte as i8)) as u16;
                let _ = write!(self.disasm, "${byte:02X}     {instr_ref} ${addr:04X}");
            }
            AddrMode::ZP0 => {
                let byte = peek_byte();
                let val = self.peek(byte.into());
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr_ref} ${byte:02X} = #${val:02X}"
                );
            }
            AddrMode::ZPX => {
                let byte = peek_byte();
                let addr = byte.wrapping_add(self.x);
                let val = self.peek(addr.into());
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr_ref} ${byte:02X},X @ ${addr:02X} = #${val:02X}"
                );
            }
            AddrMode::ZPY => {
                let byte = peek_byte();
                let addr = byte.wrapping_add(self.y);
                let val = self.peek(addr.into());
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr_ref} ${byte:02X},Y @ ${addr:02X} = #${val:02X}"
                );
            }
            AddrMode::IND => {
                let (byte1, byte2, base_addr) = peek_word();
                let val = if (base_addr & 0xFF) == 0xFF {
                    let lo = self.peek(base_addr);
                    let hi = self.peek(base_addr - 0xFF);
                    u16::from_le_bytes([lo, hi])
                } else {
                    self.peek_word(base_addr)
                };
                let _ = write!(
                    self.disasm,
                    "${byte1:02X} ${byte2:02X} {instr_ref} (${base_addr:04X}) = ${val:04X}"
                );
            }
            AddrMode::IDX => {
                let byte = peek_byte();
                let zero_addr = byte.wrapping_add(self.x);
                let lo = self.peek(u16::from(zero_addr));
                let hi = self.peek(u16::from(zero_addr.wrapping_add(1)));
                let addr = u16::from_le_bytes([lo, hi]);
                let val = self.peek(addr);
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr_ref} (${byte:02X},X) @ ${addr:04X} = #${val:02X}"
                );
            }
            AddrMode::IDY | AddrMode::IDYW => {
                let byte = peek_byte();
                let base_addr = {
                    let lo = self.peek(u16::from(byte));
                    let hi = self.peek(u16::from(byte.wrapping_add(1)));
                    u16::from_le_bytes([lo, hi])
                };
                let addr = base_addr.wrapping_add(u16::from(self.y));
                let val = self.peek(addr);
                let _ = write!(
                    self.disasm,
                    "${byte:02X}     {instr_ref} (${byte:02X}),Y @ ${addr:04X} = #${val:02X}"
                );
            }
            AddrMode::ABS => {
                let (byte1, byte2, addr) = peek_word();
                if instr_ref.instr == JMP {
                    let _ = write!(
                        self.disasm,
                        "${byte1:02X} ${byte2:02X} {instr_ref} ${addr:04X}"
                    );
                } else {
                    let val = self.peek(addr);
                    let _ = write!(
                        self.disasm,
                        "${byte1:02X} ${byte2:02X} {instr_ref} ${addr:04X} = #${val:02X}"
                    );
                }
            }
            AddrMode::ABX | AddrMode::ABXW => {
                let (byte1, byte2, base_addr) = peek_word();
                let addr = base_addr.wrapping_add(self.x.into());
                let val = self.peek(addr);
                let _ = write!(
                    self.disasm,
                    "${byte1:02X} ${byte2:02X} {instr_ref} ${base_addr:04X},X @ ${addr:04X} = #${val:02X}"
                );
            }
            AddrMode::ABY | AddrMode::ABYW => {
                let (byte1, byte2, base_addr) = peek_word();
                let addr = base_addr.wrapping_add(self.y.into());
                let val = self.peek(addr);
                let _ = write!(
                    self.disasm,
                    "${byte1:02X} ${byte2:02X} {instr_ref} ${base_addr:04X},Y @ ${addr:04X} = #${val:02X}"
                );
            }
            AddrMode::OTH => {
                let (byte1, byte2, addr) = peek_word();
                if instr_ref.instr == JSR {
                    let _ = write!(
                        self.disasm,
                        "${byte1:02X} ${byte2:02X} {instr_ref} ${addr:04X}"
                    );
                } else {
                    let val = self.peek(addr);
                    let _ = write!(
                        self.disasm,
                        "${byte1:02X} ${byte2:02X} {instr_ref} ${addr:04X} = #${val:02X}"
                    );
                }
            }
        };
        &self.disasm
    }

    /// Logs the disassembled instruction being executed.
    pub fn trace_instr(&mut self) {
        let mut pc = self.pc;
        let status = self.status;
        let acc = self.acc;
        let x = self.x;
        let y = self.y;
        let sp = self.sp;
        let ppu_cycle = self.bus.ppu.cycle;
        let ppu_scanline = self.bus.ppu.scanline;
        let cycle = self.cycle;
        let n = if status.contains(Status::N) { 'N' } else { 'n' };
        let v = if status.contains(Status::V) { 'V' } else { 'v' };
        let i = if status.contains(Status::I) { 'I' } else { 'i' };
        let z = if status.contains(Status::Z) { 'Z' } else { 'z' };
        let c = if status.contains(Status::C) { 'C' } else { 'c' };
        println!(
            "{:<50} A:{acc:02X} X:{x:02X} Y:{y:02X} P:{n}{v}--d{i}{z}{c} SP:{sp:02X} PPU:{ppu_cycle:3},{ppu_scanline:3} CYC:{cycle}",
            self.disassemble(&mut pc),
        );
    }

    // Utilities

    /// Returns whether two addresses are on different memory pages.
    #[inline(always)]
    #[must_use]
    const fn pages_differ(addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }

    /// Returns whether a memory page is crossed using relative address.
    #[inline(always)]
    #[must_use]
    const fn page_crossed(addr: u16, offset: i16) -> bool {
        ((addr as i16 + offset) as u16 & 0xFF00) != (addr & 0xFF00)
    }
}

impl Clock for Cpu {
    /// Runs the CPU one instruction.
    fn clock(&mut self, intrs: &mut CpuInterrupts) {
        #[cfg(feature = "trace")]
        self.trace_instr();

        let opcode = self.fetch_byte(intrs); // Cycle 1
        let op = Cpu::OPS[usize::from(opcode)];
        self.addr_mode = op.addr_mode();
        self.operand = self.fetch_operand(intrs);
        op.run(self, intrs);

        if self
            .irq_flags
            .intersects(IrqFlags::PREV_RUN_IRQ | IrqFlags::PREV_NMI)
        {
            self.irq(intrs);
        }
    }
}

impl Read for Cpu {
    #[inline(always)]
    fn read(&mut self, addr: u16, intrs: &mut CpuInterrupts) -> u8 {
        if intrs.halt_for_dma() {
            self.handle_dma(addr, intrs);
        }

        self.start_cycle(self.start_cycles - 1, intrs);
        let val = self.bus.read(addr, intrs);
        self.end_cycle(self.end_cycles + 1, intrs);
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        self.bus.peek(addr)
    }
}

impl Write for Cpu {
    #[inline(always)]
    fn write(&mut self, addr: u16, val: u8, intrs: &mut CpuInterrupts) {
        self.start_cycle(self.start_cycles + 1, intrs);
        self.bus.write(addr, val, intrs);
        self.end_cycle(self.end_cycles - 1, intrs);
    }
}

impl Regional for Cpu {
    #[inline(always)]
    fn region(&self) -> NesRegion {
        self.bus.region
    }

    fn set_region(&mut self, region: NesRegion, intrs: &mut CpuInterrupts) {
        let (start_cycles, end_cycles) = match region {
            NesRegion::Auto | NesRegion::Ntsc => (6, 6), // NTSC_MASTER_CLOCK_DIVIDER / 2
            NesRegion::Pal => (8, 8),                    // PAL_MASTER_CLOCK_DIVIDER / 2
            NesRegion::Dendy => (7, 8),                  // DENDY_MASTER_CLOCK_DIVIDER / 2
        };
        self.start_cycles = start_cycles;
        self.end_cycles = end_cycles;
        self.bus.set_region(region, intrs);
    }
}

impl Reset for Cpu {
    /// Resets the CPU
    ///
    /// Updates the PC, SP, and Status values to defined constants.
    ///
    /// These operations take the CPU 7 cycles.
    fn reset(&mut self, kind: ResetKind, intrs: &mut CpuInterrupts) {
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

        self.bus.reset(kind, intrs);
        self.cycle = 0;
        self.master_clock = 0;
        self.irq_flags = IrqFlags::default();
        self.corrupted = false;

        // Read directly from bus so as to not clock other components during reset
        let lo = self.bus.read(Self::RESET_VECTOR, intrs);
        let hi = self.bus.read(Self::RESET_VECTOR + 1, intrs);
        self.pc = u16::from_le_bytes([lo, hi]);

        // The CPU takes 7 cycles to reset/power on
        // See:
        // * <https://www.nesdev.org/wiki/CPU_interrupts>
        // * <http://archive.6502.org/datasheets/synertek_programming_manual.pdf>
        for _ in 0..7 {
            self.start_cycle(self.start_cycles - 1, intrs);
            self.end_cycle(self.start_cycles + 1, intrs);
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
            .field("interrupt_flags", &self.irq_flags)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::{cart::Cart, cpu::instr::Instr::*, mapper::Nrom};

    #[test]
    fn cycle_timing() {
        use super::*;
        let mut cpu = Cpu::new(Bus::default());
        let mut cart = Cart::empty();
        cart.mapper = Nrom::load(&mut cart).unwrap();
        cpu.bus.load_cart(cart);

        let mut intrs = CpuInterrupts::default();
        cpu.reset(ResetKind::Hard, &mut intrs);
        cpu.clock(&mut intrs);

        assert_eq!(cpu.cycle, 14, "cpu after power + one clock");

        for instr_ref in Cpu::INSTR_REF.iter() {
            let extra_cycle = match instr_ref.instr {
                BCC | BNE | BPL | BVC => 1,
                _ => 0,
            };
            // Ignore invalid opcodes
            if instr_ref.instr == HLT {
                continue;
            }
            cpu.reset(ResetKind::Hard, &mut intrs);
            cpu.bus.write(0x0000, instr_ref.opcode, &mut intrs);
            cpu.clock(&mut intrs);
            let cpu_cyc = u32::from(7 + instr_ref.cycles + extra_cycle);
            assert_eq!(
                cpu.cycle, cpu_cyc,
                "cpu ${:02X} {:?} #{:?}",
                instr_ref.opcode, instr_ref.instr, instr_ref.addr_mode
            );
        }
    }
}
