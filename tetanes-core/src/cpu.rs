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
use std::{
    cell::Cell,
    fmt::{self},
};
use tracing::trace;

pub mod instr;

thread_local! {
    static NMI: Cell<bool> = const { Cell::new(false) };
    static IRQS: Cell<Irq> = const { Cell::new(Irq::empty()) };
    static DMAS: Cell<Dma> = const { Cell::new(Dma::empty()) };
    static DMA_HALT: Cell<bool> = const { Cell::new(false) };
    static DMA_DUMMY_READ: Cell<bool> = const { Cell::new(false) };
    static DMA_OAM_ADDR: Cell<u16> = const { Cell::new(0x0000) };
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
    /// Total number of cycles ran. Wraps around.
    pub cycle: u32,
    /// Total number of master cycles. Used to synchronize components. Resets on NMI.
    pub master_clock: u32,
    /// Number of master clocks to cycle at the start of a read/write
    pub start_cycles: u8,
    /// Number of master clocks to cycle at the end of a read/write
    pub end_cycles: u8,
    /// Program counter.
    pub pc: u16,
    /// Opcode operand.
    pub operand: u16,
    /// Addressing mode.
    pub addr_mode: AddrMode,
    // === 16 ===
    /// Stack pointer register - stack is at $0100-$01FF.
    pub sp: u8,
    /// Accumulator register.
    pub acc: u8,
    /// X register.
    pub x: u8,
    /// Y register.
    pub y: u8,
    /// Status Registers.
    pub status: Status,
    /// IRQ flags.
    pub irq_flags: IrqFlags,
    // === 24 ==
    /// Data bus.
    pub bus: Bus,
    /// String cache for disassembly.
    #[serde(skip)]
    pub disasm: String,
    // Encountering an invalid opcode corrupts CPU processing.
    #[serde(skip)]
    pub corrupted: bool,
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
            disasm: String::new(),
            corrupted: false,
        };
        cpu.set_region(cpu.bus.region);
        cpu
    }

    /// Load a CPU state.
    pub fn load(&mut self, mut cpu: Self) {
        // Because we don't want to serialize the entire ROM in save states, extract out the
        // already loaded ROM data if it's not provided
        if cpu.bus.prg_rom.is_empty() {
            cpu.bus.prg_rom = std::mem::take(&mut self.bus.prg_rom);
        }
        if cpu.bus.ppu.bus.chr.is_empty() {
            cpu.bus.ppu.bus.chr = std::mem::take(&mut self.bus.ppu.bus.chr);
        }
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

    #[inline]
    #[must_use]
    pub fn nmi_pending() -> bool {
        NMI.get()
    }

    #[inline]
    pub fn set_nmi() {
        NMI.set(true);
    }

    #[inline]
    pub fn clear_nmi() {
        NMI.set(false);
    }

    #[inline]
    pub fn irqs() -> Irq {
        IRQS.get()
    }

    #[inline]
    #[must_use]
    pub fn has_irq(irq: Irq) -> bool {
        IRQS.get().contains(irq)
    }

    #[inline]
    pub fn set_irq(irq: Irq) {
        IRQS.set(IRQS.get() | irq);
    }

    #[inline]
    pub fn clear_irq(irq: Irq) {
        IRQS.set(IRQS.get() & !irq);
    }

    #[inline]
    pub fn start_dmc_dma() {
        DMAS.set(DMAS.get() | Dma::DMC);
        DMA_HALT.set(true);
        DMA_DUMMY_READ.set(true);
    }

    #[inline]
    pub fn start_oam_dma(addr: u16) {
        DMAS.set(DMAS.get() | Dma::OAM);
        DMA_HALT.set(true);
        DMA_OAM_ADDR.set(addr);
    }

    #[inline]
    #[must_use]
    pub fn halt_for_dma() -> bool {
        DMA_HALT.get()
    }

    #[inline]
    pub fn dma_oam_addr() -> u16 {
        DMA_OAM_ADDR.get()
    }

    #[inline]
    #[must_use]
    pub fn dmas_running() -> Option<(bool, bool)> {
        let dmas = DMAS.get();
        (!dmas.is_empty()).then_some((dmas.contains(Dma::DMC), dmas.contains(Dma::OAM)))
    }

    #[inline]
    pub fn clear_dma(dma: Dma) {
        DMAS.set(DMAS.get() & !dma);
    }

    #[inline]
    pub fn clear_dma_halt() {
        DMA_HALT.set(false);
    }

    #[inline]
    pub fn dma_dummy_read() -> bool {
        DMA_DUMMY_READ.get()
    }

    #[inline]
    pub fn clear_dma_dummy_read() {
        DMA_DUMMY_READ.set(false);
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
    pub fn irq(&mut self) {
        if Self::halt_for_dma() && self.region() == NesRegion::Pal {
            // Check for DMA on PAL
            self.handle_dma(self.pc);
        }

        self.read(self.pc); // Dummy read
        self.read(self.pc); // Dummy read
        self.push_word(self.pc);

        // Pushing status to the stack has to happen after checking NMI since it can hijack the BRK
        // IRQ when it occurs between cycles 4 and 5.
        // https://www.nesdev.org/wiki/CPU_interrupts#Interrupt_hijacking
        //
        // Set U and !B during push
        let status = ((self.status | Status::U) & !Status::B).bits();
        let nmi = self.irq_flags.intersects(IrqFlags::NMI);
        self.push_byte(status);
        self.status.set(Status::I, true);

        if nmi {
            self.irq_flags.remove(IrqFlags::NMI);
            self.pc = self.read_word(Self::NMI_VECTOR);
            self.bus.ppu.clock_to(self.master_clock);
            self.master_clock = self.master_clock.saturating_sub(self.bus.ppu.master_clock);
            self.bus.ppu.master_clock = 0;
            trace!(
                "NMI - PPU:{:3},{:3} CYC:{}",
                self.bus.ppu.cycle, self.bus.ppu.scanline, self.cycle
            );
        } else {
            self.pc = self.read_word(Self::IRQ_VECTOR);
            trace!(
                "IRQ - PPU:{:3},{:3} CYC:{}",
                self.bus.ppu.cycle, self.bus.ppu.scanline, self.cycle
            );
        }
    }

    /// Handle CPU interrupt requests, if any are pending.
    #[inline(always)]
    fn handle_interrupts(&mut self) {
        let flags = &mut self.irq_flags;

        // https://www.nesdev.org/wiki/CPU_interrupts
        //
        // The internal signal goes high during φ1 of the cycle that follows the one where
        // the edge is detected, and stays high until the NMI has been handled. NMI is handled only
        // when `prev_nmi` is true.
        flags.set(IrqFlags::PREV_NMI, flags.contains(IrqFlags::NMI));

        // This edge detector polls the status of the NMI line during φ2 of each CPU cycle (i.e.,
        // during the second half of each cycle, hence here in `end_cycle`) and raises an internal
        // signal if the input goes from being high during one cycle to being low during the
        // next.
        let nmi_pending = Self::nmi_pending();
        let prev_nmi_pending = flags.contains(IrqFlags::PREV_NMI_PENDING);
        if !prev_nmi_pending && nmi_pending {
            flags.insert(IrqFlags::NMI);
        }
        flags.set(IrqFlags::PREV_NMI_PENDING, nmi_pending);

        // The IRQ status at the end of the second-to-last cycle is what matters,
        // so keep the second-to-last status.
        flags.set(IrqFlags::PREV_RUN_IRQ, flags.contains(IrqFlags::RUN_IRQ));
        let irqs = Self::irqs();
        let run_irq = !irqs.is_empty() && !self.status.intersects(Status::I);
        flags.set(IrqFlags::RUN_IRQ, run_irq);

        #[cfg(feature = "trace")]
        if !flags.contains(IrqFlags::PREV_RUN_IRQ) && flags.contains(IrqFlags::RUN_IRQ) {
            trace!("IRQs: {:?} - CYC:{}", irqs, self.cycle);
        }
    }

    /// Start a CPU cycle.
    #[inline(always)]
    fn start_cycle(&mut self, increment: u8) {
        self.master_clock += u32::from(increment);
        self.cycle = self.cycle.wrapping_add(1);
        self.bus.clock_to(self.master_clock - Self::PPU_OFFSET);
        self.bus.clock();
    }

    /// End a CPU cycle.
    #[inline(always)]
    fn end_cycle(&mut self, increment: u8) {
        self.master_clock += u32::from(increment);
        self.bus.clock_to(self.master_clock - Self::PPU_OFFSET);

        self.handle_interrupts();
    }

    /// Start a direct-memory access (DMA) cycle.
    #[inline(always)]
    fn start_dma_cycle(&mut self) {
        // OAM DMA cycles count as halt/dummy reads for DMC DMA when both run at the same time
        if Self::halt_for_dma() {
            Self::clear_dma_halt();
        } else {
            Self::clear_dma_dummy_read();
        }
        self.start_cycle(self.start_cycles - 1);
    }

    /// Handle a direct-memory access (DMA) request.
    #[cold]
    #[inline(never)]
    fn handle_dma(&mut self, addr: u16) {
        trace!("Starting DMA - CYC:{}", self.cycle);

        self.start_cycle(self.start_cycles - 1);
        self.bus.read(addr);
        self.end_cycle(self.start_cycles + 1);
        Self::clear_dma_halt();

        let skip_dummy_reads = addr == 0x4016 || addr == 0x4017;

        let mut oam_offset = 0;
        let mut oam_dma_count = 0;
        let mut read_val = 0;

        while let Some((dmc_dma, oam_dma)) = Self::dmas_running() {
            if self.cycle & 0x01 == 0x00 {
                if dmc_dma && !Self::halt_for_dma() && !Self::dma_dummy_read() {
                    // DMC DMA ready to read a byte (halt and dummy read done before)
                    self.start_dma_cycle();
                    let dma_addr = self.bus.apu.dmc.dma_addr();
                    read_val = self.bus.read(dma_addr);
                    trace!(
                        "Loaded DMC DMA byte. ${dma_addr:04X}: {read_val} - CYC:{}",
                        self.cycle
                    );
                    self.end_cycle(self.start_cycles + 1);
                    self.bus.apu.dmc.load_buffer(read_val);
                    Self::clear_dma(Dma::DMC);
                } else if oam_dma {
                    // DMC DMA not running or ready, run OAM DMA
                    self.start_dma_cycle();
                    read_val = self.bus.read(Self::dma_oam_addr() + oam_offset);
                    self.end_cycle(self.start_cycles + 1);
                    oam_offset += 1;
                    oam_dma_count += 1;
                } else {
                    // DMC DMA running, but not ready yet (needs to halt, or dummy read) and OAM
                    // DMA isn't running
                    debug_assert!(Self::halt_for_dma() || Self::dma_dummy_read());
                    self.start_dma_cycle();
                    if !skip_dummy_reads {
                        self.bus.read(addr); // throw away
                    }
                    self.end_cycle(self.start_cycles + 1);
                }
            } else if oam_dma && oam_dma_count & 0x01 == 0x01 {
                // OAM DMA write cycle, done on odd cycles after a read on even cycles
                self.start_dma_cycle();
                self.bus.write(0x2004, read_val);
                self.end_cycle(self.start_cycles + 1);
                oam_dma_count += 1;
                if oam_dma_count == 0x200 {
                    Self::clear_dma(Dma::OAM);
                }
            } else {
                // Align to read cycle before starting OAM DMA (or align to perform DMC read)
                self.start_dma_cycle();
                if !skip_dummy_reads {
                    self.bus.read(addr); // throw away
                }
                self.end_cycle(self.start_cycles + 1);
            }
        }
    }

    // Status Register functions

    /// Set [`Status`] flags for the given bits.
    #[inline(always)]
    fn set_status(&mut self, status: Status) {
        self.status = status & !Status::U & !Status::B;
    }

    /// Returns the [`Status`] register as a byte.
    #[inline(always)]
    const fn status_bits(&self, reg: Status) -> u8 {
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
    fn push_byte(&mut self, val: u8) {
        self.write(Self::SP_BASE | u16::from(self.sp), val);
        self.sp = self.sp.wrapping_sub(1);
    }

    /// Pull a byte from the stack.
    #[inline(always)]
    #[must_use]
    fn pop_byte(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.read(Self::SP_BASE | u16::from(self.sp))
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
    fn push_word(&mut self, val: u16) {
        let [lo, hi] = val.to_le_bytes();
        self.push_byte(hi);
        self.push_byte(lo);
    }

    /// Pull a word (two bytes) from the stack
    #[inline(always)]
    fn pop_word(&mut self) -> u16 {
        let lo = self.pop_byte();
        let hi = self.pop_byte();
        u16::from_le_bytes([lo, hi])
    }

    // Memory accesses

    /// Fetch a byte and increments PC by 1.
    #[inline(always)]
    #[must_use]
    fn fetch_byte(&mut self) -> u8 {
        let val = self.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        val
    }

    /// Fetch opcode operand based on addressing mode.
    #[inline(always)]
    #[must_use]
    fn fetch_operand(&mut self) -> u16 {
        match self.addr_mode {
            AddrMode::ACC | AddrMode::IMP => self.acc_imp(),
            AddrMode::IMM | AddrMode::REL | AddrMode::ZP0 => self.imm_rel_zp(),
            AddrMode::ZPX => self.zpx(),
            AddrMode::ZPY => self.zpy(),
            AddrMode::IND => self.ind(),
            AddrMode::IDX => self.idx(),
            AddrMode::IDY => self.idy(false),
            AddrMode::IDYW => self.idy(true),
            AddrMode::ABS => self.abs(),
            AddrMode::ABX => self.abx(false),
            AddrMode::ABXW => self.abx(true),
            AddrMode::ABY => self.aby(false),
            AddrMode::ABYW => self.aby(true),
            AddrMode::OTH => 0,
        }
    }

    /// Fetch a 16-bit word and increments PC by 2.
    #[inline(always)]
    #[must_use]
    fn fetch_word(&mut self) -> u16 {
        let lo = self.fetch_byte();
        let hi = self.fetch_byte();
        u16::from_le_bytes([lo, hi])
    }

    /// Read operand value.
    #[inline(always)]
    #[must_use]
    fn read_operand(&mut self) -> u8 {
        if matches!(
            self.addr_mode,
            AddrMode::ACC | AddrMode::IMP | AddrMode::IMM | AddrMode::REL
        ) {
            self.operand as u8
        } else {
            self.read(self.operand)
        }
    }

    /// Read a 16-bit word.
    #[inline(always)]
    #[must_use]
    pub fn read_word(&mut self, addr: u16) -> u16 {
        let lo = self.read(addr);
        let hi = self.read(addr.wrapping_add(1));
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
        }
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
    fn clock(&mut self) {
        #[cfg(feature = "trace")]
        self.trace_instr();

        let opcode = self.fetch_byte(); // Cycle 1
        let op = Cpu::OPS[usize::from(opcode)];
        self.addr_mode = op.addr_mode();
        self.operand = self.fetch_operand();
        op.run(self);

        if self
            .irq_flags
            .intersects(IrqFlags::PREV_RUN_IRQ | IrqFlags::PREV_NMI)
        {
            self.irq();
        }
    }
}

impl Read for Cpu {
    #[inline(always)]
    fn read(&mut self, addr: u16) -> u8 {
        if Self::halt_for_dma() {
            self.handle_dma(addr);
        }

        self.start_cycle(self.start_cycles - 1);
        let val = self.bus.read(addr);
        self.end_cycle(self.end_cycles + 1);
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        self.bus.peek(addr)
    }
}

impl Write for Cpu {
    #[inline(always)]
    fn write(&mut self, addr: u16, val: u8) {
        self.start_cycle(self.start_cycles + 1);
        self.bus.write(addr, val);
        self.end_cycle(self.end_cycles - 1);
    }
}

impl Regional for Cpu {
    #[inline(always)]
    fn region(&self) -> NesRegion {
        self.bus.region
    }

    fn set_region(&mut self, region: NesRegion) {
        let (start_cycles, end_cycles) = match region {
            NesRegion::Auto | NesRegion::Ntsc => (6, 6), // NTSC_MASTER_CLOCK_DIVIDER / 2
            NesRegion::Pal => (8, 8),                    // PAL_MASTER_CLOCK_DIVIDER / 2
            NesRegion::Dendy => (7, 8),                  // DENDY_MASTER_CLOCK_DIVIDER / 2
        };
        self.start_cycles = start_cycles;
        self.end_cycles = end_cycles;
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

        self.master_clock = 0;
        self.cycle = 0;
        self.irq_flags = IrqFlags::default();
        self.bus.reset(kind);
        self.corrupted = false;
        Self::clear_nmi();
        Self::clear_irq(Irq::all());
        Self::clear_dma_halt();
        Self::clear_dma(Dma::all());
        Self::clear_dma_dummy_read();

        // Read directly from bus so as to not clock other components during reset
        let lo = self.bus.read(Self::RESET_VECTOR);
        let hi = self.bus.read(Self::RESET_VECTOR + 1);
        self.pc = u16::from_le_bytes([lo, hi]);

        // The CPU takes 7 cycles to reset/power on
        // See:
        // * <https://www.nesdev.org/wiki/CPU_interrupts>
        // * <http://archive.6502.org/datasheets/synertek_programming_manual.pdf>
        for _ in 0..7 {
            self.start_cycle(self.start_cycles - 1);
            self.end_cycle(self.start_cycles + 1);
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
            .field("irqs", &Self::irqs())
            .field("interrupt_flags", &self.irq_flags)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::{cart::Cart, cpu::instr::Instr::*, mapper::Nrom, mem::RamState};

    #[test]
    fn cycle_timing() {
        use super::*;
        let mut cpu = Cpu::new(Bus {
            ram_state: RamState::AllZeros,
            ..Bus::default()
        });
        let mut cart = Cart {
            ram_state: RamState::AllZeros,
            ..Cart::empty()
        };
        cart.mapper = Nrom::load(&mut cart).unwrap();
        cpu.bus.load_cart(cart);
        cpu.reset(ResetKind::Hard);
        cpu.clock();

        assert_eq!(cpu.cycle, 14, "cpu after power + one clock");

        for instr_ref in Cpu::INSTR_REF.iter() {
            #[allow(
                clippy::wildcard_enum_match_arm,
                reason = "only branch instructions have an extra cycle"
            )]
            let extra_cycle = match instr_ref.instr {
                BCC | BNE | BPL | BVC => 1,
                _ => 0,
            };
            // Ignore invalid opcodes
            if instr_ref.instr == HLT {
                continue;
            }
            cpu.reset(ResetKind::Hard);
            cpu.bus.write(0x0000, instr_ref.opcode);
            cpu.clock();
            let cpu_cyc = u32::from(7 + instr_ref.cycles + extra_cycle);
            assert_eq!(
                cpu.cycle, cpu_cyc,
                "cpu ${:02X} {:?} #{:?}",
                instr_ref.opcode, instr_ref.instr, instr_ref.addr_mode
            );
        }
    }
}
