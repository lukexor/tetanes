//! 6502 Central Processing Unit (CPU) implementation.
//!
//! <https://wiki.nesdev.org/w/index.php/CPU>
//!
//! [`Cpu`] is the register file and cycle counters - the state a 6502 keeps. Everything it *does*
//! is an inherent method on [`Bus`], since a memory access moves the whole console: reading a byte
//! clocks the PPU, the APU and the board on the way past. Those `impl Bus` blocks are here and in
//! [`instr`].
//!
//! # Stability
//!
//! [`Cpu`]'s fields are the emulation's internal wiring - registers and cycle counters. They are
//! public so that embedders and debuggers can read them, but they track the implementation rather
//! than the crate version, and a release may add, rename or retype any of them. The stable entry
//! point is [`ControlDeck`](crate::control_deck::ControlDeck). See the crate-level
//! [stability](crate#stability) note for the tier this belongs to and why.

use crate::{
    bus::Bus,
    common::{NesRegion, ResetKind},
    debug::{Access, AccessHit, ByteKind, Verdict},
};
use crate::{
    cpu::instr::{
        AddrMode,
        Instr::{JMP, JSR, SYA},
        InstrRef,
    },
    mapper::MapperOps,
};
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::fmt::{self};
use tracing::trace;

pub mod instr;

bitflags! {
    /// Interrupt and DMA state, kept as one byte because it is tested on every cycle.
    ///
    /// The `PREV_` flags are the previous cycle's values, which is what makes an interrupt take
    /// effect one instruction late the way the hardware does.
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone)]
    #[must_use]
    pub struct IrqFlags: u8 {
        /// NMI line asserted.
        const NMI = 1 << 0;
        /// NMI line as of the previous cycle.
        const PREV_NMI = 1 << 1;
        /// NMI was pending as of the previous cycle.
        const PREV_NMI_PENDING = 1 << 2;
        /// An IRQ will be serviced after the current instruction.
        const RUN_IRQ = 1 << 3;
        /// `RUN_IRQ` as of the previous cycle.
        const PREV_RUN_IRQ = 1 << 4;
        /// A DMC sample fetch is stealing cycles.
        const DMA_DMC = 1 << 5;
        /// The CPU is halted for a DMA.
        const DMA_HALT = 1 << 6;
        /// The DMA's alignment dummy read.
        const DMA_DUMMY_READ = 1 << 7;
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
    #[derive(Default, Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
    #[must_use]
    pub struct Status: u8 {
        /// Carry.
        const C = 1;
        /// Zero.
        const Z = 1 << 1;
        /// Disable interrupt.
        const I = 1 << 2;
        /// Decimal mode; unused by the NES, but the flag itself still works.
        const D = 1 << 3;
        /// Break: set when pushed by PHP/BRK, clear when pushed by IRQ/NMI.
        const B = 1 << 4;
        /// Unused; always set when pushed to the stack.
        const U = 1 << 5;
        /// Overflow.
        const V = 1 << 6;
        /// Negative.
        const N = 1 << 7;
    }
}
/// Returned by [`Bus::load_state`] when a save state was not produced by the loaded cart.
///
/// A save state carries no ROM - it is reattached from the running console - so it carries the
/// ROM's CRC32 instead, and a state recorded against another game cannot be applied at all.
#[derive(thiserror::Error, Debug, Copy, Clone, PartialEq, Eq)]
#[error("save state does not match the loaded ROM")]
#[must_use]
pub struct StateMismatch;

/// One disassembled instruction, split into the parts a view tints and aligns separately.
///
/// [`Display`](fmt::Display) joins them back into the single line the instruction trace prints,
/// which is also the column layout the debugger draws.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[must_use]
pub struct Disasm {
    /// Address the instruction was read from.
    pub addr: u16,
    /// Opcode, mnemonic, addressing mode and cycle count.
    pub instr: InstrRef,
    /// The bytes after the opcode. [`Disasm::operands`] slices off the ones that belong to the
    /// instruction.
    pub bytes: [u8; 2],
    /// The operand as written: `#$42`, `$1234,X`, `($10),Y`. Empty for implied and accumulator
    /// modes, which name no operand.
    pub operand: String,
    /// The address the operand lands on once a register is added, for the modes that compute one.
    ///
    /// `None` where the operand already names its address, and for the modes that touch memory
    /// not at all.
    pub effective: Option<u16>,
    /// What sits at the address the operand reaches, on the console as it stands.
    pub value: Option<Resolved>,
}

/// What an operand comes to when it is followed.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum Resolved {
    /// The byte the instruction reads or writes.
    Byte(u8),
    /// The address an indirect jump reads out of the one it names.
    Word(u16),
}

impl fmt::Display for Resolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Byte(byte) => write!(f, "#${byte:02X}"),
            Self::Word(word) => write!(f, "${word:04X}"),
        }
    }
}

impl Disasm {
    /// Columns the operand bytes are padded to, which `$10 $00 ` fills at its widest.
    pub const BYTE_COLUMNS: usize = 8;

    /// The bytes after the opcode that belong to the instruction, zero to two of them.
    pub fn operands(&self) -> &[u8] {
        &self.bytes[..usize::from(self.instr.addr_mode.operand_len())]
    }

    /// How many bytes the instruction occupies, opcode included.
    #[expect(
        clippy::len_without_is_empty,
        reason = "an instruction is never zero bytes"
    )]
    pub const fn len(&self) -> u16 {
        1 + self.instr.addr_mode.operand_len() as u16
    }

    /// [`Disasm::effective`] as either renderer writes it, two hex digits in zero page and four
    /// everywhere else.
    pub fn effective_text(&self) -> Option<String> {
        let effective = self.effective?;
        Some(
            if matches!(self.instr.addr_mode, AddrMode::ZPX | AddrMode::ZPY) {
                format!("${effective:02X}")
            } else {
                format!("${effective:04X}")
            },
        )
    }
}

impl fmt::Display for Disasm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${:04X} ${:02X} ", self.addr, self.instr.opcode)?;
        let mut columns = 0;
        for byte in self.operands() {
            write!(f, "${byte:02X} ")?;
            columns += 4;
        }
        // Padded so the mnemonic starts in the same column whatever the instruction's length.
        write!(f, "{:width$}", "", width = Self::BYTE_COLUMNS - columns)?;
        write!(f, "{}", self.instr)?;
        if !self.operand.is_empty() {
            write!(f, " {}", self.operand)?;
        }
        // `@ addr` rather than the debugger's `[addr]`, since this line is read side by side with
        // other emulators' instruction traces.
        if let Some(effective) = self.effective_text() {
            write!(f, " @ {effective}")?;
        }
        if let Some(value) = self.value {
            write!(f, " = {value}")?;
        }
        Ok(())
    }
}

/// The Central Processing Unit status and registers
#[derive(Default, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(C)]
pub struct Cpu {
    /// Total CPU cycles run.
    pub cycle: u32,
    /// Master clock cycles run, which is what the PPU and APU are caught up against.
    pub master_clock: u32,
    /// Program counter.
    pub pc: u16,
    /// The current instruction's operand.
    pub operand: u16,
    /// Master cycles before a read or write within the current instruction.
    pub start_cycles: u8,
    /// Master cycles after a read or write within the current instruction.
    pub end_cycles: u8,
    /// The current instruction's addressing mode.
    pub addr_mode: AddrMode,
    /// Stack pointer; the stack itself is at $0100-$01FF.
    pub sp: u8,
    /// Accumulator.
    pub acc: u8,
    /// X index register.
    pub x: u8,
    /// Y index register.
    pub y: u8,
    /// Status register.
    pub status: Status,
    /// Interrupt and DMA state.
    pub irq_flags: IrqFlags,
    /// Source page of an OAM DMA in progress.
    pub dma_oam_addr: Option<u16>,
    /// Set when an invalid opcode has jammed the CPU; it keeps cycling but stops fetching.
    #[serde(skip)]
    pub corrupted: bool,
}

impl Cpu {
    /// NTSC Master Clock Rate in Hz.
    pub const NTSC_MASTER_CLOCK_RATE: f32 = 21_477_272.0;
    /// NTSC CPU Clock Rate in Hz.
    pub const NTSC_CPU_CLOCK_RATE: f32 = Self::NTSC_MASTER_CLOCK_RATE / 12.0;
    /// PAL Master Clock Rate in Hz.
    pub const PAL_MASTER_CLOCK_RATE: f32 = 26_601_712.0;
    /// PAL CPU Clock Rate in Hz.
    pub const PAL_CPU_CLOCK_RATE: f32 = Self::PAL_MASTER_CLOCK_RATE / 16.0;
    /// Dendy CPU Clock Rate in Hz.
    pub const DENDY_CPU_CLOCK_RATE: f32 = Self::PAL_MASTER_CLOCK_RATE / 15.0;

    // Represents CPU/PPU alignment and would range from 1..=ppu.clock_divider-1
    // if random PPU alignment was emulated
    // See: https://www.nesdev.org/wiki/PPU_frame_timing#CPU-PPU_Clock_Alignment
    pub(crate) const PPU_OFFSET: u32 = 1;

    /// NMI vector Address.
    pub const NMI_VECTOR: u16 = 0xFFFA;
    /// IRQ vector address
    pub const IRQ_VECTOR: u16 = 0xFFFE;
    /// Reset vector address.
    pub const RESET_VECTOR: u16 = 0xFFFC;
    /// Power on status register values.
    pub const POWER_ON_STATUS: Status = Status::U.union(Status::I);
    /// Power on stack pointer value.
    pub const POWER_ON_SP: u8 = 0xFD;
    /// Stack Pointer base address.
    pub const SP_BASE: u16 = 0x0100;

    /// `JSR` opcode, jumps to location, save return address.
    pub const JSR: u8 = 0x20;
    /// `RTI` opcode, return from interrupt.
    pub const RTI: u8 = 0x40;
    /// `RTS` opcode, return from subroutine.
    pub const RTS: u8 = 0x60;

    /// Create a new CPU timed for `region`.
    pub fn new(region: NesRegion) -> Self {
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
            dma_oam_addr: None,
            corrupted: false,
        };
        cpu.set_region(region);
        cpu
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

    /// Re-times the CPU for `region`.
    ///
    /// Only the cycle lengths; [`Bus::set_region`] is what forwards the change to the rest of the
    /// console.
    pub const fn set_region(&mut self, region: NesRegion) {
        let (start_cycles, end_cycles) = match region {
            NesRegion::Auto | NesRegion::Ntsc => (6, 6), // NTSC_MASTER_CLOCK_DIVIDER / 2
            NesRegion::Pal => (8, 8),                    // PAL_MASTER_CLOCK_DIVIDER / 2
            NesRegion::Dendy => (7, 8),                  // DENDY_MASTER_CLOCK_DIVIDER / 2
        };
        self.start_cycles = start_cycles;
        self.end_cycles = end_cycles;
    }

    /// Resets the registers.
    ///
    /// Updates the SP and Status values to defined constants. [`Bus::reset`] is what resets the
    /// rest of the console, fetches the reset vector into PC, and runs the seven cycles the reset
    /// itself takes.
    pub fn reset(&mut self, kind: ResetKind) {
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

        self.cycle = 0;
        self.master_clock = 0;
        self.irq_flags = IrqFlags::default();
        self.corrupted = false;
    }

    /// Start OAM DMA.
    #[inline]
    pub const fn start_oam_dma(&mut self, addr: u16) {
        self.irq_flags = self.irq_flags.union(IrqFlags::DMA_HALT);
        self.dma_oam_addr = Some(addr);
    }

    // Interrupt flag functions

    /// Clear [`IrqFlags`] flags for the given bits.
    #[inline(always)]
    pub(crate) const fn clear_irq_flags(&mut self, flags: IrqFlags) {
        self.irq_flags = self.irq_flags.difference(flags);
    }

    /// Returns `true` if the [`IrqFlags`] register is set.
    #[inline(always)]
    pub(crate) const fn irq_flags(&self, flags: IrqFlags) -> bool {
        self.irq_flags.intersection(flags).bits() == flags.bits()
    }

    // Status Register functions

    /// Set [`Status`] flags for the given bits.
    #[inline(always)]
    pub(crate) const fn set_status(&mut self, status: Status) {
        self.status = status.difference(Status::U).difference(Status::B);
    }

    /// Returns the [`Status`] register as a byte.
    #[inline(always)]
    pub(crate) const fn status_bit(&self, reg: Status) -> u8 {
        self.status.intersection(reg).bits()
    }

    /// Set accumulator and update [`Status`] flags based on value.
    #[inline(always)]
    pub(crate) fn set_acc(&mut self, val: u8) {
        self.set_zn_status(val);
        self.acc = val;
    }

    /// Set x and update [`Status`] flags based on value.
    #[inline(always)]
    pub(crate) fn set_x(&mut self, val: u8) {
        self.set_zn_status(val);
        self.x = val;
    }

    /// Set y and update [`Status`] flags based on value.
    #[inline(always)]
    pub(crate) fn set_y(&mut self, val: u8) {
        self.set_zn_status(val);
        self.y = val;
    }

    /// Set stack pointer.
    #[inline(always)]
    pub(crate) const fn set_sp(&mut self, val: u8) {
        self.sp = val;
    }

    /// Set both [`Status::Z`] and [`Status::N`] flags based on value.
    #[inline(always)]
    pub(crate) fn set_zn_status(&mut self, val: u8) {
        self.status.set(Status::Z, val == 0x00);
        self.status.set(Status::N, val & 0x80 > 0);
    }

    // Utilities

    /// Returns whether two addresses are on different memory pages.
    #[inline(always)]
    #[must_use]
    pub(crate) const fn pages_differ(addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }

    /// Returns whether a memory page is crossed using relative address.
    #[inline(always)]
    #[must_use]
    pub(crate) const fn page_crossed(addr: u16, offset: i16) -> bool {
        ((addr as i16 + offset) as u16 & 0xFF00) != (addr & 0xFF00)
    }
}

/// The CPU's view of the console: every access below moves the whole machine, so it takes the
/// [`Bus`] rather than the [`Cpu`].
impl Bus {
    /// Load a console state, leaving `self` untouched if it does not belong to this cart.
    ///
    /// A state is the emulated machine and nothing else, so this is also what puts back what one
    /// cannot carry: the cart's ROM, reattached from the running console, the attached debugger,
    /// and the settings that belong to the player rather than to the NES.
    ///
    /// # Errors
    ///
    /// If the state was not produced by the currently loaded cart.
    pub fn load_state(&mut self, mut state: Self) -> Result<(), StateMismatch> {
        // Checked before anything moves out of the running console, so that a state from another
        // cart leaves it exactly as it was.
        if !self.memory.is_same_cart(&state.memory) {
            return Err(StateMismatch);
        }
        // A state recorded elsewhere - a file, a rewind buffer - carries the settings the player
        // had when it was recorded. Those are the session's, not the machine's, and must not come
        // back with it. See `Bus::keep_session_settings`.
        state.keep_session_settings(self);
        self.swap_state(&mut state)
    }

    /// Exchange the console for `state`, leaving what was running here in `state`.
    ///
    /// Every restore path ends here - [`Bus::load_state`], rewind, and run-ahead, which calls it
    /// directly - so it is what puts back what a state must not carry: the cart's ROM, copied in
    /// from the running console, and the attached debugger. It is also where a state belonging to
    /// a different cart is refused.
    ///
    /// Run-ahead restores a snapshot of *this* console taken a few frames ago, which is why it
    /// stops short of `Bus::keep_session_settings`: the settings are already its own, and the APU
    /// filter and synthesiser history that would come across with them belongs to the timeline
    /// being discarded. Swapping rather than assigning also hands it back the console it gave up,
    /// whose allocations it snapshots into again next frame.
    ///
    /// # Errors
    ///
    /// If the state was not produced by the currently loaded cart.
    pub(crate) fn swap_state(&mut self, state: &mut Self) -> Result<(), StateMismatch> {
        if !state.memory.restore_rom_from(&self.memory) {
            return Err(StateMismatch);
        }
        state.debugger = std::mem::take(&mut self.debugger);
        state.debugger_active = self.debugger_active;
        // What a debugger has recorded belongs to the session, not to the state. Restoring
        // previous state does not obsolete which bytes are instructions.
        // The cart is the same one - `restore_rom_from` above asserted that - so the map's offsets
        // still address the same bytes.
        state.pc_history = self.pc_history.take();
        state.code_map = self.code_map.take();
        // Breakpoints belong to the session too, and what they caught belongs to the timeline
        // being discarded. Run-ahead restores over speculative frames, so a hit from one of those
        // would otherwise stop the console at a PC it never reached.
        state.breakpoints_active = self.breakpoints_active;
        state.breakpoints = self.breakpoints.take();
        state.access_hit = None;
        // The pixel path compares against thresholds derived from $2001 rather than reading its
        // flags, and they are not part of the save format.
        state.ppu.update_draw_thresholds();
        std::mem::swap(self, state);
        Ok(())
    }

    /// Move the settings the *player* owns out of the running console and into a state about to
    /// replace it.
    ///
    /// A save state or a rewind snapshot is the emulated machine, but `Bus` also holds a handful
    /// of knobs that belong to the session rather than to the NES: emulation speed, the audio
    /// device's sample rate, muted channels, cheats, the headless flags. Restoring those along
    /// with the machine rewinds the player's settings too.
    ///
    /// That was not cosmetic. Emulation speed is split across two fields - `ControlDeck` counts
    /// how many NES frames to clock, while [`Apu::speed`](crate::apu::Apu::speed) stretches the sample period - and only
    /// the second is inside `Bus`. Rewinding past a stretch of fast-forward therefore left the APU
    /// at 2x while the deck clocked at 1x, so half the expected samples were produced per frame,
    /// the audio queue never filled, and a frontend pacing itself against that queue never waited.
    /// Fast-forward appeared to switch itself back on and stick.
    ///
    /// Deliberately *not* included is [`NesRegion`]: unlike these, it
    /// changes what the machine is, and every counter in the state was produced under the region
    /// the state carries. A state is restored with the region it was recorded at, and
    /// [`ControlDeck::set_region`](crate::control_deck::ControlDeck::set_region) is what changes
    /// it afterwards.
    ///
    /// Note that `#[serde(skip)]` does *not* protect a setting from this. The restore is
    /// `*self = state`, so a skipped field arrives as `Default` rather than keeping the value the
    /// running console had - which is how `ram_state` below silently reverted on every load.
    fn keep_session_settings(&mut self, session: &mut Self) {
        // Emulation speed and the output sample rate, plus the sample period derived from them.
        self.apu.speed = session.apu.speed;
        self.apu.sample_rate = session.apu.sample_rate;
        self.apu.sample_ratio = session.apu.sample_ratio;
        // Swapped rather than rebuilt for the same reason as the filter chain below: it holds
        // steps still being played out, and it is already tuned to the session's rate.
        std::mem::swap(&mut self.apu.synth, &mut session.apu.synth);
        // Swapped rather than rebuilt: the chain's contents are signal history, so carrying the
        // running one over keeps audio continuous across a restore instead of restarting the
        // filters mid-waveform. It is configured for the session's rate, which is what the three
        // fields above just restored. (If the region changed since the state was recorded, the
        // chain is one region behind until `Apu::set_region` next rebuilds it - the same tradeoff
        // the region note above describes.)
        std::mem::swap(&mut self.apu.filter_chain, &mut session.apu.filter_chain);

        // Channels the player muted, which live beside the hardware's own enable bits.
        self.apu.pulse1.set_silent(session.apu.pulse1.silent());
        self.apu.pulse2.set_silent(session.apu.pulse2.silent());
        self.apu.triangle.set_silent(session.apu.triangle.silent());
        self.apu.noise.set_silent(session.apu.noise.silent());
        self.apu.dmc.set_silent(session.apu.dmc.silent());
        self.apu.mapper_enabled = session.apu.mapper_enabled;

        // Headless flags, and PPU warmup emulation.
        self.apu.skip_mixing = session.apu.skip_mixing;
        self.ppu.skip_rendering = session.ppu.skip_rendering;
        self.ppu.emulate_warmup = session.ppu.emulate_warmup;

        // Cheats: a code entered after the state was recorded stays entered.
        std::mem::swap(&mut self.patches, &mut session.patches);

        // How RAM is filled at power-on, which only shows on the next hard reset.
        self.ram_state = session.ram_state;

        // What is plugged in, and how it behaves - not what is currently pressed, which is
        // emulated state the game reads.
        self.input.set_four_player(session.input.four_player);
        self.input
            .set_concurrent_dpad(session.input.joypads[0].concurrent_dpad);
        self.input.zapper.connected = session.input.zapper.connected;
        self.input.zapper.trigger_release_delay = session.input.zapper.trigger_release_delay;
        self.input.zapper.radius = session.input.zapper.radius;
    }

    /// Clock rate based on currently configured NES region.
    #[inline]
    #[must_use]
    pub const fn clock_rate(&self) -> f32 {
        Cpu::region_clock_rate(self.region)
    }

    /// Peek at the next instruction.
    #[inline]
    pub fn next_instr(&self) -> InstrRef {
        let opcode = self.peek(self.cpu.pc);
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
    pub fn irq(&mut self) {
        if self.cpu.irq_flags(IrqFlags::DMA_HALT) && self.region == NesRegion::Pal {
            // Check for DMA on PAL
            self.handle_dma(self.cpu.pc);
        }

        self.read_unwatched(self.cpu.pc); // Dummy read
        self.read_unwatched(self.cpu.pc); // Dummy read
        self.push_word(self.cpu.pc);

        // Pushing status to the stack has to happen after checking NMI since it can hijack the BRK
        // IRQ when it occurs between cycles 4 and 5.
        // https://www.nesdev.org/wiki/CPU_interrupts#Interrupt_hijacking
        //
        // Set U and !B during push
        let status = ((self.cpu.status | Status::U) & !Status::B).bits();
        let nmi = self.cpu.irq_flags(IrqFlags::NMI);
        self.push_byte(status);
        self.cpu.status.set(Status::I, true);

        if nmi {
            self.cpu.clear_irq_flags(IrqFlags::NMI);
            self.cpu.pc = self.read_word(Cpu::NMI_VECTOR);
            self.clock_sync();
            trace!(
                "NMI - PPU:{:3},{:3} CYC:{}",
                self.ppu.cycle, self.ppu.scanline, self.cpu.cycle
            );
        } else {
            self.cpu.pc = self.read_word(Cpu::IRQ_VECTOR);
            trace!(
                "IRQ - PPU:{:3},{:3} CYC:{}",
                self.ppu.cycle, self.ppu.scanline, self.cpu.cycle
            );
        }
    }

    /// Handle CPU interrupt requests, if any are pending.
    #[inline(always)]
    fn handle_interrupts(&mut self) {
        let mapper_ops = self.mapper_ops;
        let irq_pending_mapper = mapper_ops.intersects(MapperOps::IRQ) && self.mapper.irq_pending();
        let dma_pending_mapper = mapper_ops.intersects(MapperOps::DMA) && self.mapper.dma_pending();
        let nmi_pending = self.ppu.nmi_pending;
        let irq_pending_apu = self.apu.irq_pending();
        let dma_pending_apu = self.apu.dma_pending();

        if dma_pending_apu {
            self.apu.clear_dma_pending();
            self.cpu
                .irq_flags
                .insert(IrqFlags::DMA_DMC | IrqFlags::DMA_HALT | IrqFlags::DMA_DUMMY_READ);
        } else if dma_pending_mapper {
            self.mapper.clear_dma_pending();
            self.cpu
                .irq_flags
                .insert(IrqFlags::DMA_DMC | IrqFlags::DMA_HALT | IrqFlags::DMA_DUMMY_READ);
        }

        let status = self.cpu.status;
        let flags = &mut self.cpu.irq_flags;

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
        let prev_nmi_pending = flags.contains(IrqFlags::PREV_NMI_PENDING);
        if !prev_nmi_pending & nmi_pending {
            flags.insert(IrqFlags::NMI);
        }
        flags.set(IrqFlags::PREV_NMI_PENDING, nmi_pending);

        // The IRQ status at the end of the second-to-last cycle is what matters,
        // so keep the second-to-last status.
        flags.set(IrqFlags::PREV_RUN_IRQ, flags.contains(IrqFlags::RUN_IRQ));
        let run_irq = (irq_pending_mapper | irq_pending_apu) & !status.intersects(Status::I);
        flags.set(IrqFlags::RUN_IRQ, run_irq);

        #[cfg(feature = "trace")]
        if !flags.contains(IrqFlags::PREV_NMI_PENDING) && flags.contains(IrqFlags::RUN_IRQ) {
            trace!(
                "IRQ: {} - CYC:{}",
                irq_pending_mapper | irq_pending_apu,
                self.cpu.cycle
            );
        }
    }

    /// Start a CPU cycle.
    #[inline(always)]
    pub(crate) fn start_cycle(&mut self, increment: u8) {
        self.cpu.master_clock = self.cpu.master_clock.wrapping_add(u32::from(increment));
        self.cpu.cycle = self.cpu.cycle.wrapping_add(1);
        self.ppu_clock_to(self.cpu.master_clock - Cpu::PPU_OFFSET);
        self.cpu_clock();
    }

    /// End a CPU cycle.
    #[inline(always)]
    pub(crate) fn end_cycle(&mut self, increment: u8) {
        self.cpu.master_clock = self.cpu.master_clock.wrapping_add(u32::from(increment));
        self.ppu_clock_to(self.cpu.master_clock - Cpu::PPU_OFFSET);

        self.handle_interrupts();
    }

    /// Start a direct-memory access (DMA) cycle.
    #[inline(always)]
    fn start_dma_cycle(&mut self) {
        // OAM DMA cycles count as halt/dummy reads for DMC DMA when both run at the same time
        if self.cpu.irq_flags(IrqFlags::DMA_HALT) {
            self.cpu.clear_irq_flags(IrqFlags::DMA_HALT);
        } else {
            self.cpu.clear_irq_flags(IrqFlags::DMA_DUMMY_READ);
        }
        self.start_cycle(self.cpu.start_cycles - 1);
    }

    /// Handle a direct-memory access (DMA) request.
    #[cold]
    #[inline(never)]
    fn handle_dma(&mut self, addr: u16) {
        trace!("Starting DMA - CYC:{}", self.cpu.cycle);

        self.start_cycle(self.cpu.start_cycles - 1);
        self.cpu_bus_read(addr);
        self.end_cycle(self.cpu.start_cycles + 1);
        self.cpu.clear_irq_flags(IrqFlags::DMA_HALT);

        let skip_dummy_reads = addr == 0x4016 || addr == 0x4017;

        let mut oam_offset = 0;
        let mut oam_dma_count = 0;
        let mut read_val = 0;

        loop {
            let dma_dmc = self.cpu.irq_flags(IrqFlags::DMA_DMC);
            let dma_oam_addr = self.cpu.dma_oam_addr;
            if !dma_dmc & dma_oam_addr.is_none() {
                break;
            }

            if self.cpu.cycle & 0x01 == 0x00 {
                if dma_dmc
                    & !self.cpu.irq_flags(IrqFlags::DMA_HALT)
                    & !self.cpu.irq_flags(IrqFlags::DMA_DUMMY_READ)
                {
                    // DMC DMA ready to read a byte (halt and dummy read done before)
                    self.start_dma_cycle();
                    let dma_addr = self.apu.dmc.dma_addr();
                    read_val = self.cpu_bus_read(dma_addr);
                    trace!(
                        "Loaded DMC DMA byte. ${dma_addr:04X}: {read_val} - CYC:{}",
                        self.cpu.cycle
                    );
                    self.end_cycle(self.cpu.start_cycles + 1);
                    self.apu.dmc.load_buffer(read_val);
                    self.cpu.clear_irq_flags(IrqFlags::DMA_DMC);
                } else if let Some(oam_addr) = dma_oam_addr {
                    // DMC DMA not running or ready, run OAM DMA
                    self.start_dma_cycle();
                    read_val = self.cpu_bus_read(oam_addr + oam_offset);
                    self.end_cycle(self.cpu.start_cycles + 1);
                    oam_offset += 1;
                    oam_dma_count += 1;
                } else {
                    // DMC DMA running, but not ready yet (needs to halt, or dummy read) and OAM
                    // DMA isn't running
                    debug_assert!(
                        self.cpu.irq_flags(IrqFlags::DMA_HALT)
                            | self.cpu.irq_flags(IrqFlags::DMA_DUMMY_READ)
                    );
                    self.start_dma_cycle();
                    if !skip_dummy_reads {
                        self.cpu_bus_read(addr); // throw away
                    }
                    self.end_cycle(self.cpu.start_cycles + 1);
                }
            } else if dma_oam_addr.is_some() & (oam_dma_count & 0x01 == 0x01) {
                // OAM DMA write cycle, done on odd cycles after a read on even cycles
                self.start_dma_cycle();
                self.cpu_bus_write(0x2004, read_val);
                self.end_cycle(self.cpu.start_cycles + 1);
                oam_dma_count += 1;
                if oam_dma_count == 0x200 {
                    self.cpu.dma_oam_addr.take();
                }
            } else {
                // Align to read cycle before starting OAM DMA (or align to perform DMC read)
                self.start_dma_cycle();
                if !skip_dummy_reads {
                    self.cpu_bus_read(addr); // throw away
                }
                self.end_cycle(self.cpu.start_cycles + 1);
            }
        }
    }

    // Stack Functions

    /// Push a byte to the stack.
    #[inline(always)]
    pub(crate) fn push_byte(&mut self, val: u8) {
        self.write(Cpu::SP_BASE | u16::from(self.cpu.sp), val);
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
    }

    /// Pull a byte from the stack.
    #[inline(always)]
    #[must_use]
    pub(crate) fn pop_byte(&mut self) -> u8 {
        self.cpu.sp = self.cpu.sp.wrapping_add(1);
        self.read(Cpu::SP_BASE | u16::from(self.cpu.sp))
    }

    /// Peek byte at the top of the stack.
    #[inline]
    #[must_use]
    pub fn peek_stack(&self) -> u8 {
        self.peek(Cpu::SP_BASE | u16::from(self.cpu.sp.wrapping_add(1)))
    }

    /// Peek at the top of the stack.
    #[inline]
    #[must_use]
    pub fn peek_stack_u16(&self) -> u16 {
        let lo = self.peek(Cpu::SP_BASE | u16::from(self.cpu.sp));
        let hi = self.peek(Cpu::SP_BASE | u16::from(self.cpu.sp.wrapping_add(1)));
        u16::from_le_bytes([lo, hi])
    }

    /// Push a word (two bytes) to the stack
    #[inline(always)]
    pub(crate) fn push_word(&mut self, val: u16) {
        let [lo, hi] = val.to_le_bytes();
        self.push_byte(hi);
        self.push_byte(lo);
    }

    /// Pull a word (two bytes) from the stack
    #[inline(always)]
    pub(crate) fn pop_word(&mut self) -> u16 {
        let lo = self.pop_byte();
        let hi = self.pop_byte();
        u16::from_le_bytes([lo, hi])
    }

    // Memory accesses

    /// Read a byte, spending a full CPU cycle - which clocks the PPU, the APU and the board.
    #[inline(always)]
    pub fn read(&mut self, addr: u16) -> u8 {
        let val = self.read_unwatched(addr);
        if self.breakpoints_active {
            self.check_access(addr, Access::READ, val);
        }
        val
    }

    /// Read a byte without a read breakpoint seeing it, spending its cycle all the same.
    ///
    /// The 6502 spends cycles on reads the program did not ask for: a wrong address before the
    /// high byte is fixed on an indexed page cross, a re-read before a read-modify-write, and the
    /// dummy reads of PC or the stack that a branch, `JSR`, `RTS`, `RTI`, `PLA` and `PLP` take
    /// while the address bus has nothing else to do. All of them come through here, so a read
    /// breakpoint reports what the program reached for. Instruction and operand fetches do too,
    /// since [`Access::EXEC`] covers those and a read breakpoint over a code bank would otherwise
    /// fire on every instruction in it.
    #[inline(always)]
    pub fn read_unwatched(&mut self, addr: u16) -> u8 {
        if self.cpu.irq_flags(IrqFlags::DMA_HALT) {
            self.handle_dma(addr);
        }

        self.start_cycle(self.cpu.start_cycles - 1);
        let val = self.cpu_bus_read(addr);
        self.end_cycle(self.cpu.end_cycles + 1);
        val
    }

    /// Record an access against the armed breakpoints, keeping the first one that stops.
    ///
    /// Out of line and cold, so the test in `read` and `write` is a predictable branch over a
    /// call rather than the scan itself.
    #[cold]
    #[inline(never)]
    fn check_access(&mut self, addr: u16, access: Access, val: u8) {
        let pc = self.instr_addr;
        let hit = AccessHit {
            pc,
            addr,
            access,
            value: val,
        };
        let verdict = self.breakpoint_verdict(addr, access);
        if verdict.record
            && let Some(breakpoints) = self.breakpoints.as_mut()
        {
            breakpoints.record(hit);
        }
        if verdict.stop && self.access_hit.is_none() {
            self.access_hit = Some(hit);
        }
    }

    /// Ask the armed breakpoints about an access, with the whole console in hand.
    #[inline]
    fn breakpoint_verdict(&self, addr: u16, access: Access) -> Verdict {
        self.breakpoints
            .as_ref()
            .map_or_else(Verdict::default, |breakpoints| {
                breakpoints.check(self, addr, access)
            })
    }

    /// Record the instruction at `pc` against the breakpoints that watch execution.
    ///
    /// Only the breakpoints that record are served here. Stopping on execution belongs to whoever
    /// drives the console, through
    /// [`ControlDeck::clock_frame_until`](crate::control_deck::ControlDeck::clock_frame_until),
    /// because a stop part way through an instruction has no clean way to unwind. So
    /// [`Verdict::stop`] is dropped here and a breakpoint that stops is checked between
    /// instructions instead.
    ///
    /// Out of line and behind the bitmap, so an instruction nothing watches tests one bit.
    #[cold]
    #[inline(never)]
    fn check_exec(&mut self, pc: u16) {
        if self.breakpoint_verdict(pc, Access::EXEC).record {
            let opcode = self.peek(pc);
            if let Some(breakpoints) = self.breakpoints.as_mut() {
                breakpoints.record(AccessHit {
                    pc,
                    addr: pc,
                    access: Access::EXEC,
                    value: opcode,
                });
            }
        }
    }

    /// Read a byte without side effects, and without moving the console.
    #[inline(always)]
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.cpu_bus_peek(addr)
    }

    /// Write a byte, spending a full CPU cycle - which clocks the PPU, the APU and the board.
    #[inline(always)]
    pub fn write(&mut self, addr: u16, val: u8) {
        self.write_unwatched(addr, val);
        if self.breakpoints_active {
            self.check_access(addr, Access::WRITE, val);
        }
    }

    /// Write a byte without a write breakpoint seeing it, spending its cycle all the same.
    ///
    /// A read-modify-write puts the old value back before the new one. Reporting that would name
    /// a value the program never chose, and `check_access` keeps the first hit, so the report
    /// would be the wrong one.
    #[inline(always)]
    pub fn write_unwatched(&mut self, addr: u16, val: u8) {
        self.start_cycle(self.cpu.start_cycles + 1);
        if addr == 0x4014 {
            self.cpu.start_oam_dma(u16::from(val) << 8);
        } else {
            self.cpu_bus_write(addr, val);
        }
        self.end_cycle(self.cpu.end_cycles - 1);
    }

    /// Fetch a byte and increments PC by 1.
    #[inline(always)]
    #[must_use]
    pub(crate) fn fetch_byte(&mut self) -> u8 {
        let val = self.read_unwatched(self.cpu.pc);
        self.cpu.pc = self.cpu.pc.wrapping_add(1);
        val
    }

    /// Fetch opcode operand based on addressing mode.
    #[inline(always)]
    #[must_use]
    fn fetch_operand(&mut self) -> u16 {
        match self.cpu.addr_mode {
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
    pub(crate) fn fetch_word(&mut self) -> u16 {
        let lo = self.fetch_byte();
        let hi = self.fetch_byte();
        u16::from_le_bytes([lo, hi])
    }

    /// Read operand value.
    #[inline(always)]
    #[must_use]
    pub(crate) fn read_operand(&mut self) -> u8 {
        if matches!(
            self.cpu.addr_mode,
            AddrMode::ACC | AddrMode::IMP | AddrMode::IMM | AddrMode::REL
        ) {
            self.cpu.operand as u8
        } else {
            self.read(self.cpu.operand)
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

    /// Disassemble the instruction at the given program counter, advancing `pc` past it.
    pub fn disassemble(&self, pc: &mut u16) -> Disasm {
        let mut disasm = Disasm::default();
        self.disassemble_into(pc, &mut disasm);
        disasm
    }

    /// Disassemble the instruction at the given program counter into `out`, advancing `pc` past it.
    ///
    /// `out` is reused rather than returned, so a sweep over the address space grows its string
    /// once instead of allocating per instruction.
    pub fn disassemble_into(&self, pc: &mut u16, out: &mut Disasm) {
        use fmt::Write;

        out.addr = *pc;
        out.operand.clear();
        out.effective = None;
        out.value = None;

        let opcode = {
            let byte = self.peek(*pc);
            *pc = pc.wrapping_add(1);
            byte
        };
        out.instr = Cpu::INSTR_REF[usize::from(opcode)];

        let mut peek_byte = |out: &mut Disasm, index: usize| {
            let byte = self.peek(*pc);
            *pc = pc.wrapping_add(1);
            out.bytes[index] = byte;
            byte
        };
        let mut peek_word = |out: &mut Disasm| {
            let lo = peek_byte(out, 0);
            let hi = peek_byte(out, 1);
            u16::from_le_bytes([lo, hi])
        };

        // `out.operand` gives the operand as written, `out.effective` and `out.value` what it
        // comes to on the console as it stands now. Only the row at PC is about to run, so the
        // rest resolve against registers they will not see.
        match out.instr.addr_mode {
            AddrMode::ACC | AddrMode::IMP => (),
            AddrMode::IMM => {
                let byte = peek_byte(out, 0);
                let _ = write!(out.operand, "#${byte:02X}");
            }
            AddrMode::REL => {
                let byte = peek_byte(out, 0);
                let addr = (*pc as i16).wrapping_add(i16::from(byte as i8)) as u16;
                let _ = write!(out.operand, "${addr:04X}");
            }
            AddrMode::ZP0 => {
                let byte = peek_byte(out, 0);
                let _ = write!(out.operand, "${byte:02X}");
                out.value = Some(Resolved::Byte(self.peek(byte.into())));
            }
            AddrMode::ZPX => {
                let byte = peek_byte(out, 0);
                let addr = byte.wrapping_add(self.cpu.x);
                let _ = write!(out.operand, "${byte:02X},X");
                out.effective = Some(addr.into());
                out.value = Some(Resolved::Byte(self.peek(addr.into())));
            }
            AddrMode::ZPY => {
                let byte = peek_byte(out, 0);
                let addr = byte.wrapping_add(self.cpu.y);
                let _ = write!(out.operand, "${byte:02X},Y");
                out.effective = Some(addr.into());
                out.value = Some(Resolved::Byte(self.peek(addr.into())));
            }
            AddrMode::IND => {
                let base_addr = peek_word(out);
                let val = if (base_addr & 0xFF) == 0xFF {
                    let lo = self.peek(base_addr);
                    let hi = self.peek(base_addr - 0xFF);
                    u16::from_le_bytes([lo, hi])
                } else {
                    self.peek_word(base_addr)
                };
                let _ = write!(out.operand, "(${base_addr:04X})");
                out.value = Some(Resolved::Word(val));
            }
            AddrMode::IDX => {
                let byte = peek_byte(out, 0);
                let zero_addr = byte.wrapping_add(self.cpu.x);
                let lo = self.peek(u16::from(zero_addr));
                let hi = self.peek(u16::from(zero_addr.wrapping_add(1)));
                let addr = u16::from_le_bytes([lo, hi]);
                let _ = write!(out.operand, "(${byte:02X},X)");
                out.effective = Some(addr);
                out.value = Some(Resolved::Byte(self.peek(addr)));
            }
            AddrMode::IDY | AddrMode::IDYW => {
                let byte = peek_byte(out, 0);
                let base_addr = {
                    let lo = self.peek(u16::from(byte));
                    let hi = self.peek(u16::from(byte.wrapping_add(1)));
                    u16::from_le_bytes([lo, hi])
                };
                let addr = base_addr.wrapping_add(u16::from(self.cpu.y));
                let _ = write!(out.operand, "(${byte:02X}),Y");
                out.effective = Some(addr);
                out.value = Some(Resolved::Byte(self.peek(addr)));
            }
            AddrMode::ABS => {
                let addr = peek_word(out);
                let _ = write!(out.operand, "${addr:04X}");
                // A jump names where it goes, so the byte sitting at the target says nothing about
                // it.
                if out.instr.instr != JMP {
                    out.value = Some(Resolved::Byte(self.peek(addr)));
                }
            }
            // JSR shares the catch-all mode with the unofficial stores, and is the one of them
            // that reaches an absolute address.
            AddrMode::OTH if out.instr.instr == JSR => {
                let addr = peek_word(out);
                let _ = write!(out.operand, "${addr:04X}");
            }
            // The rest of the catch-all is the unofficial stores, which index the way
            // `InstrRef::mode_name` reports: SYA by X, the other three by Y.
            AddrMode::ABX | AddrMode::ABXW | AddrMode::ABY | AddrMode::ABYW | AddrMode::OTH => {
                let base_addr = peek_word(out);
                let indexed_by_x = matches!(out.instr.addr_mode, AddrMode::ABX | AddrMode::ABXW)
                    || out.instr.instr == SYA;
                let (index, register) = if indexed_by_x {
                    ('X', self.cpu.x)
                } else {
                    ('Y', self.cpu.y)
                };
                let addr = base_addr.wrapping_add(register.into());
                let _ = write!(out.operand, "${base_addr:04X},{index}");
                out.effective = Some(addr);
                out.value = Some(Resolved::Byte(self.peek(addr)));
            }
        };
    }

    /// Logs the disassembled instruction being executed.
    #[cold]
    #[inline(never)]
    pub fn trace_instr(&mut self) {
        if !tracing::enabled!(tracing::Level::TRACE) {
            return;
        }
        let mut pc = self.cpu.pc;
        let status = self.cpu.status;
        let acc = self.cpu.acc;
        let x = self.cpu.x;
        let y = self.cpu.y;
        let sp = self.cpu.sp;
        let ppu_cycle = self.ppu.cycle;
        let ppu_scanline = self.ppu.scanline;
        let cycle = self.cpu.cycle;
        let n = if status.contains(Status::N) { 'N' } else { 'n' };
        let v = if status.contains(Status::V) { 'V' } else { 'v' };
        let i = if status.contains(Status::I) { 'I' } else { 'i' };
        let z = if status.contains(Status::Z) { 'Z' } else { 'z' };
        let c = if status.contains(Status::C) { 'C' } else { 'c' };
        let disasm = self.disassemble(&mut pc).to_string();
        println!(
            "{disasm:<50} A:{acc:02X} X:{x:02X} Y:{y:02X} P:{n}{v}--d{i}{z}{c} SP:{sp:02X} PPU:{ppu_cycle:3},{ppu_scanline:3} CYC:{cycle}",
        );
    }

    /// Runs all components up to master clock, synchronizing them.
    #[inline(always)]
    pub fn clock_sync(&mut self) {
        self.ppu_clock_to(self.cpu.master_clock);
        self.cpu.master_clock = self.cpu.master_clock.saturating_sub(self.ppu.master_clock);
        self.ppu.master_clock = 0;
        self.apu.clock_sync();
    }

    /// Runs the CPU one instruction.
    #[inline(always)]
    pub fn clock_instr(&mut self) {
        #[cfg(feature = "trace")]
        self.trace_instr();

        let prev_pc = self.cpu.pc;
        if let Some(history) = &mut self.pc_history {
            history.push(prev_pc);
        }
        if let Some(code_map) = &mut self.code_map
            && let Some(offset) = self.memory.prg_offset(prev_pc)
        {
            code_map.mark(offset, ByteKind::CODE);
        }
        // An access is caught part way through, by which point PC has moved into the operand. A
        // hit that named that would point at no instruction at all.
        if self.breakpoints_active {
            self.instr_addr = prev_pc;
            self.check_exec(prev_pc);
        }
        let opcode = self.fetch_byte(); // Cycle 1
        let op = Cpu::OPS[usize::from(opcode)];
        self.cpu.addr_mode = op.addr_mode();
        self.cpu.operand = self.fetch_operand();
        op.run(self);

        // A JSR destination is wherever it left PC, so this reads after the opcode has run and
        // before an interrupt can change it again. The next instruction would mark it as code,
        // but this additionally marks it as a subroutine entry.
        if let Some(code_map) = &mut self.code_map
            && opcode == Cpu::JSR
            && let Some(offset) = self.memory.prg_offset(self.cpu.pc)
        {
            code_map.mark(offset, ByteKind::SUB_ENTRY);
        }

        if self
            .cpu
            .irq_flags
            .intersects(IrqFlags::PREV_RUN_IRQ | IrqFlags::PREV_NMI)
        {
            self.irq();
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
            .field("interrupt_flags", &self.irq_flags)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::{cart::Cart, cpu::instr::Instr::*, mapper::Nrom};

    /// The parts have to join back into the line the instruction trace prints, which is read
    /// against other emulators' logs, and into the columns the debugger draws. One instruction per
    /// addressing mode, plus the cases that turn a part off: a jump resolves nothing, and an
    /// unofficial opcode takes the `*` the mnemonic column reserves. All four unofficial stores
    /// are here, since the catch-all mode they share names no index of its own.
    #[test]
    fn the_parts_join_back_into_one_aligned_line() {
        use super::*;
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        cart.mapper = Nrom::load(&mut cart).unwrap();
        bus.load_cart(cart);
        bus.reset(ResetKind::Hard);
        bus.cpu.x = 0x02;
        bus.cpu.y = 0x03;
        for (addr, val) in [
            (0x0010u16, 0x34u8),
            (0x0011, 0x12),
            (0x0012, 0x78),
            (0x0013, 0x56),
            (0x0234, 0xCC),
            (0x0237, 0xBB),
        ] {
            bus.cpu_bus_write(addr, val);
        }

        for (bytes, expected) in [
            (&[0x0Au8] as &[u8], "$0700 $0A          ASL"),
            (&[0xEA], "$0700 $EA          NOP"),
            (&[0xA9, 0x42], "$0700 $A9 $42      LDA #$42"),
            (&[0x10, 0x7F], "$0700 $10 $7F      BPL $0781"),
            (&[0x10, 0xF0], "$0700 $10 $F0      BPL $06F2"),
            (&[0xA5, 0x10], "$0700 $A5 $10      LDA $10 = #$34"),
            (&[0xB5, 0x10], "$0700 $B5 $10      LDA $10,X @ $12 = #$78"),
            (&[0xB6, 0x10], "$0700 $B6 $10      LDX $10,Y @ $13 = #$56"),
            (
                &[0x6C, 0x10, 0x00],
                "$0700 $6C $10 $00  JMP ($0010) = $1234",
            ),
            (
                &[0xA1, 0x0E],
                "$0700 $A1 $0E      LDA ($0E,X) @ $1234 = #$CC",
            ),
            (
                &[0xB1, 0x10],
                "$0700 $B1 $10      LDA ($10),Y @ $1237 = #$BB",
            ),
            (
                &[0x91, 0x10],
                "$0700 $91 $10      STA ($10),Y @ $1237 = #$BB",
            ),
            (&[0xAD, 0x34, 0x12], "$0700 $AD $34 $12  LDA $1234 = #$CC"),
            (&[0x4C, 0x34, 0x12], "$0700 $4C $34 $12  JMP $1234"),
            (
                &[0xBD, 0x34, 0x12],
                "$0700 $BD $34 $12  LDA $1234,X @ $1236 = #$00",
            ),
            (
                &[0x9D, 0x34, 0x12],
                "$0700 $9D $34 $12  STA $1234,X @ $1236 = #$00",
            ),
            (
                &[0xB9, 0x34, 0x12],
                "$0700 $B9 $34 $12  LDA $1234,Y @ $1237 = #$BB",
            ),
            (
                &[0x99, 0x34, 0x12],
                "$0700 $99 $34 $12  STA $1234,Y @ $1237 = #$BB",
            ),
            (&[0x20, 0x34, 0x12], "$0700 $20 $34 $12  JSR $1234"),
            (
                &[0x9B, 0x34, 0x12],
                "$0700 $9B $34 $12 *TAS $1234,Y @ $1237 = #$BB",
            ),
            (
                &[0x9C, 0x34, 0x12],
                "$0700 $9C $34 $12 *SYA $1234,X @ $1236 = #$00",
            ),
            (
                &[0x9E, 0x34, 0x12],
                "$0700 $9E $34 $12 *SXA $1234,Y @ $1237 = #$BB",
            ),
            (
                &[0x9F, 0x34, 0x12],
                "$0700 $9F $34 $12 *SHAA $1234,Y @ $1237 = #$BB",
            ),
            (&[0x07, 0x10], "$0700 $07 $10     *SLO $10 = #$34"),
            (&[0x1A], "$0700 $1A         *NOP"),
        ] {
            for (i, byte) in bytes.iter().enumerate() {
                bus.cpu_bus_write(0x0700 + i as u16, *byte);
            }
            let mut pc = 0x0700;
            let disasm = bus.disassemble(&mut pc);
            assert_eq!(disasm.to_string(), expected);
            assert_eq!(
                usize::from(disasm.len()),
                bytes.len(),
                "${:02X} reported the wrong length for {expected}",
                bytes[0]
            );
        }
    }

    /// `AddressSpace::capture` steps by [`Disasm::len`], so an instruction the CPU measures
    /// differently puts the sweep out of step with every boundary after it.
    ///
    /// Measured against execution rather than against the disassembler, which derives its own
    /// length from the same [`AddrMode`] and would agree with itself whatever the CPU does.
    #[test]
    fn every_opcodes_length_matches_what_executing_it_consumes() {
        use super::*;
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        cart.mapper = Nrom::load(&mut cart).unwrap();
        bus.load_cart(cart);

        for instr_ref in Cpu::INSTR_REF.iter() {
            // A jam never leaves the instruction, and a branch or jump moves PC by its own rules.
            if matches!(
                instr_ref.instr,
                HLT | JMP | JSR | RTS | RTI | BRK | BCC | BCS | BEQ | BMI | BNE | BPL | BVC | BVS
            ) {
                continue;
            }
            bus.reset(ResetKind::Hard);
            bus.cpu_bus_write(0x0000, instr_ref.opcode);
            bus.cpu.pc = 0x0000;
            // Read off before running it, since a read-modify-write can land on its own operand.
            let mut pc = 0x0000;
            let disasm = bus.disassemble(&mut pc);
            bus.clock_instr();
            let consumed = bus.cpu.pc;
            assert_eq!(
                disasm.len(),
                consumed,
                "${:02X} {:?} #{:?} runs {consumed} bytes",
                instr_ref.opcode,
                instr_ref.instr,
                instr_ref.addr_mode
            );
        }
    }

    #[test]
    fn cycle_timing() {
        use super::*;
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        cart.mapper = Nrom::load(&mut cart).unwrap();
        bus.load_cart(cart);
        bus.reset(ResetKind::Hard);
        bus.clock_instr();

        assert_eq!(bus.cpu.cycle, 14, "cpu after power + one clock");

        for instr_ref in Cpu::INSTR_REF.iter() {
            let extra_cycle = match instr_ref.instr {
                BCC | BNE | BPL | BVC => 1,
                _ => 0,
            };
            // Ignore invalid opcodes
            if instr_ref.instr == HLT {
                continue;
            }
            bus.reset(ResetKind::Hard);
            bus.cpu_bus_write(0x0000, instr_ref.opcode);
            bus.clock_instr();
            let cpu_cyc = u32::from(7 + instr_ref.cycles + extra_cycle);
            assert_eq!(
                bus.cpu.cycle, cpu_cyc,
                "cpu ${:02X} {:?} #{:?}",
                instr_ref.opcode, instr_ref.instr, instr_ref.addr_mode
            );
        }
    }
}
