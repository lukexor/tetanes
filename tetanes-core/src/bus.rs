//! NES Memory/Data Bus implementation.
//!
//! <https://wiki.nesdev.org/w/index.php/CPU_memory_map>
//!
//! [`Bus`] owns the console: the CPU, PPU, APU, input and the cartridge's board, plus the work RAM
//! and open-bus state it routes with. It is also the unit of emulated state - the whole of what a
//! save state, a rewind frame and a run-ahead snapshot contain, and nothing else.
//!
//! Behavior that needs more than one component lives in an `impl Bus` block in the file that owns
//! the state it reads: the CPU's in [`cpu`](crate::cpu), the instruction set's in
//! [`instr`](crate::cpu::instr), the PPU's in [`ppu`](crate::ppu), the input ports' in
//! [`input`](crate::input). What is here is the CPU-side routing - which address reaches which
//! component.
//!
//! `Bus` carries both address spaces, so its accessors name the one they mean:
//!
//! | | reads | writes |
//! |---|---|---|
//! | CPU, spending a cycle | [`Bus::read`], [`Bus::peek`] | [`Bus::write`] |
//! | PPU address space | [`Bus::ppu_bus_peek`] | |
//! | cartridge, through the page tables | [`Bus::chr_peek`] | |
//!
//! [`Bus::copy_ppu_bus`] copies `$0000-$2FFF` as currently banked, for reading CHR off-thread.
//!
//! # Stability
//!
//! [`Bus`]'s fields are the emulation's internal wiring - the components it routes to, and the
//! open-bus and region state it routes with. They are public so that embedders and debuggers can
//! reach the component tree, but they track the implementation rather than the crate version, and
//! a release may add, rename or retype any of them. The stable entry point is
//! [`ControlDeck`](crate::control_deck::ControlDeck), which reaches the whole component tree
//! through [`ControlDeck::bus`](crate::control_deck::ControlDeck::bus) and
//! [`bus_mut`](crate::control_deck::ControlDeck::bus_mut). See the crate-level
//! [stability](crate#stability) note for the tier this belongs to and why.

use crate::{
    apu::{Apu, Channel},
    cart::Cart,
    common::{NesRegion, ResetKind},
    cpu::Cpu,
    debug::{Debugger, PcHistory},
    fs,
    genie::GenieCode,
    input::{Input, Player},
    mapper::{Mapper, MapperOps},
    memory::{ConstArray, Memory, RamState},
    patch::{Patch, Patches},
    ppu::Ppu,
};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use tracing::trace;

/// NES Bus
///
/// <https://wiki.nesdev.org/w/index.php/CPU_memory_map>
///
/// |-----------------| $FFFF |-----------------|
/// | PRG-ROM         |       |                 |
/// |-----------------| $8000 |-----------------|
/// | PRG-RAM or SRAM |       | PRG-RAM or SRAM |
/// |-----------------| $6000 |-----------------|
/// | Expansion       |       | Expansion       |
/// | Modules         |       | Modules         |
/// |-----------------| $4020 |-----------------|
/// | APU/Input       |       |                 |
/// | Registers       |       |                 |
/// |- - - - - - - - -| $4000 |                 |
/// | PPU Mirrors     |       | I/O Registers   |
/// | $2000-$2007     |       |                 |
/// |- - - - - - - - -| $2008 |                 |
/// | PPU Registers   |       |                 |
/// |-----------------| $2000 |-----------------|
/// | WRAM Mirrors    |       |                 |
/// | $0000-$07FF     |       |                 |
/// |- - - - - - - - -| $0800 |                 |
/// | WRAM            |       | 2K Internal     |
/// |- - - - - - - - -| $0200 | Work RAM        |
/// | Stack           |       |                 |
/// |- - - - - - - - -| $0100 |                 |
/// | Zero Page       |       |                 |
/// |-----------------| $0000 |-----------------|
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(C)]
pub struct Bus {
    /// Central Processing Unit registers and cycle counters. What the CPU *does* is an
    /// `impl Bus` block in [`cpu`](crate::cpu), since every access moves the whole console.
    pub cpu: Cpu,
    /// Which of the loaded board's optional hooks apply: a per-cycle clock, IRQ or DMA, audio,
    /// watching every PPU bus address, or serving reads itself rather than from page tables.
    ///
    /// *Derived* from [`Map::mapper_ops`](crate::mapper::Map::mapper_ops) and not serialized;
    /// writing it from outside desynchronizes dispatch from the board that is loaded.
    //
    // Cached so the hot paths gate each hook on a bit test instead of dispatching into every
    // board unconditionally. Recomputed in `load_mapper` and `rebuild_mapper_state`.
    #[serde(skip)]
    pub mapper_ops: MapperOps,
    /// Picture Processing Unit.
    pub ppu: Ppu,
    /// The cartridge's board.
    //
    // Ordered after `ppu`: the PPU is the heaviest user (CHR and CIRAM fetches are the hot path),
    // but the CPU reaches PRG through it too.
    pub mapper: Mapper,
    /// Page-table addressed cartridge memory - every region a cart has.
    pub memory: Memory,
    /// Audio Processing Unit.
    pub apu: Apu,
    /// Joypad and Zapper inputs.
    pub input: Input,
    /// 2K of work RAM on the console itself, at $0000-$07FF and mirrored to $1FFF.
    //
    // Measured un-boxed (embedded directly in `Bus`): ~1.2% slower on the bench corpus, not
    // faster, despite removing a pointer chase - inlining it grows `Bus`'s footprint enough to
    // outweigh that. Keep it boxed.
    pub wram: Box<ConstArray<u8, { size::WRAM }>>,
    /// Whatever was last read or written to to the Bus.
    pub open_bus: u8,
    /// RAM initialization state.
    #[serde(skip)]
    pub ram_state: RamState,
    /// NES Region.
    pub region: NesRegion,
    /// Whether a [`Debugger`] is attached, cached so the per-dot path can skip touching the
    /// (cold) `debugger` field when nothing is attached.
    #[serde(skip)]
    pub debugger_active: bool,
    /// Attached debugger, run at a chosen PPU dot.
    // Don't save debug state
    #[serde(skip)]
    pub debugger: Debugger,
    /// Ring buffer of executed program counters, recorded only while a debugger is open and asks
    /// for history.
    #[serde(skip)]
    pub pc_history: Option<PcHistory>,
    /// Scratch buffer for [`Bus::disassemble`].
    #[serde(skip)]
    pub disasm: String,
    /// Cheats: values substituted for what a read would otherwise return.
    ///
    /// Not serialized. A cheat is the player's current choice rather than the machine's - the same
    /// argument [`ControlDeck::load_bus`](crate::control_deck::ControlDeck::load_bus) makes for
    /// mapper revisions - and `keep_session_settings` swaps the running table back in over
    /// whatever a restore brought, so a serialized copy would be written and then discarded.
    //
    // Last, with the other cold fields, even though the WRAM read path consults its page mask:
    // measured beside `wram` instead, so that the mask shares a line with it, at 1.946 ms/frame
    // against 1.896 here and 1.881 with no patch table at all. Fifty-six bytes displacing
    // `open_bus` costs more than the mask's own load saves.
    #[serde(skip)]
    pub patches: Patches,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(NesRegion::default(), RamState::default())
    }
}

pub mod size {
    //! Memory size constants.

    /// 2K of work RAM on the console itself, available to the CPU.
    pub const WRAM: usize = 0x800;
}

impl Bus {
    /// Creates a console timed for `region`, with work RAM initialised per `ram_state`.
    pub fn new(region: NesRegion, ram_state: RamState) -> Self {
        Self {
            wram: Box::new(ConstArray::new()),
            cpu: Cpu::new(region),
            ppu: Ppu::new(region),
            mapper: Mapper::none(),
            memory: Memory::default(),
            mapper_ops: MapperOps::empty(),
            apu: Apu::new(region),
            input: Input::new(region),
            patches: Patches::default(),
            open_bus: 0x00,
            ram_state,
            region,
            debugger: Debugger::default(),
            debugger_active: false,
            pc_history: None,
            disasm: String::new(),
        }
    }

    /// Installs a cart: its board and every memory region it came with.
    pub fn load_cart(&mut self, cart: Cart) {
        self.memory = cart.memory;
        self.load_mapper(cart.mapper);
    }

    /// Removes the cart, leaving the console with no board.
    pub fn unload_cart(&mut self) {
        self.load_mapper(Mapper::default());
    }

    /// Overwrites this console with `src`'s state, reusing the allocations it already holds.
    ///
    /// What run-ahead wants every frame is not a fresh `Bus` but the one it snapshotted into last
    /// frame, refilled: a console that has already run this cart has an arena of the right size
    /// whose ROM half is already correct, so only what a frame can actually change has to be
    /// copied. Restoring is `Bus::swap_state`, which hands the console back for the next one.
    ///
    /// The debugger and the disassembly scratch are not copied, for the same reason `swap_state`
    /// moves them across a restore: they belong to the session rather than to the state.
    pub(crate) fn snapshot_from(&mut self, src: &Self) {
        // Destructured exhaustively so that a field added to `Bus` is a compile error here rather
        // than console state that silently fails to survive a run-ahead frame.
        let Self {
            cpu,
            ppu,
            mapper_ops,
            mapper,
            memory,
            apu,
            input,
            wram,
            patches,
            open_bus,
            ram_state,
            region,
            debugger_active,
            // These are all session-specific values and are not restored across snapshots.
            debugger: _,
            pc_history: _,
            disasm: _,
        } = src;

        self.cpu.clone_from(cpu);
        // Copies the 120 KiB frame buffer with it. Skipping that needs a field-wise copy of `Ppu`,
        // which belongs next to `Ppu` rather than here; the pixels themselves are dead either way,
        // since the caller parks the frame it means to display before snapshotting.
        self.ppu.clone_from(ppu);
        self.mapper_ops = *mapper_ops;
        self.mapper.clone_from(mapper);
        self.memory.snapshot_from(memory);
        self.apu.clone_from(apu);
        self.input.clone_from(input);
        self.wram.clone_from(wram);
        self.patches.clone_from(patches);
        self.open_bus = *open_bus;
        self.ram_state = *ram_state;
        self.region = *region;
        self.debugger_active = *debugger_active;
    }

    /// Copies the PPU's address space - `$0000-$2FFF`, banked and mirrored as it is right now -
    /// into `dst`.
    ///
    /// Page tables and any reads the board synthesises are resolved here, so the copy is what the
    /// PPU would fetch and the caller needs no knowledge of the board. Side-effect free: it will
    /// not move an MMC2 CHR latch or an MMC3 A12 counter.
    pub fn copy_ppu_bus(&self, dst: &mut [u8]) {
        for (addr, byte) in dst.iter_mut().enumerate().take(0x3000) {
            *byte = self.chr_peek(addr as u16);
        }
    }

    /// The console's 2K of work RAM.
    #[must_use]
    #[inline]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn wram(&self) -> &[u8; size::WRAM] {
        &self.wram
    }

    /// The console's 2K of work RAM, for a debugger or a frontend's cheat engine to write.
    #[must_use]
    #[inline]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn wram_mut(&mut self) -> &mut [u8; size::WRAM] {
        &mut self.wram
    }

    /// Apply a Game Genie code, replacing any patch already at its address.
    pub fn add_genie_code(&mut self, genie_code: GenieCode) {
        self.patches.insert(Patch::from(&genie_code));
    }

    /// Remove the patch a Game Genie code applies, if the code is one this build can read.
    pub fn remove_genie_code(&mut self, code: &str) {
        if let Ok(genie_code) = GenieCode::new(code.to_string()) {
            self.patches.remove(genie_code.addr());
        }
    }

    /// Remove every patch.
    pub fn clear_genie_codes(&mut self) {
        self.patches.clear();
    }

    /// Samples the APU has mixed since the last clear.
    #[inline]
    #[must_use]
    pub fn audio_samples(&self) -> &[f32] {
        &self.apu.audio_samples
    }

    /// Drops the mixed samples, which the clocking API does at the start of each call.
    #[inline]
    pub fn clear_audio_samples(&mut self) {
        self.apu.audio_samples.clear();
    }

    /// Clocks everything the CPU's cycle drives: the board, the APU, input and the PPU.
    // `inline(always)` rather than a hint: with only `#[inline]` the `MapperOps` bit tests below
    // push this just over LLVM's automatic inlining threshold, so it stops being inlined into
    // `Cpu::start_cycle` and each free bit test becomes a real out-of-line call every CPU cycle.
    #[inline(always)]
    pub fn cpu_clock(&mut self) {
        let ops = self.mapper_ops;
        if ops.intersects(MapperOps::CLOCKED) {
            self.mapper.clock();
        }
        // Only a board with audio records anything: the buffer it writes into stays all zeroes
        // otherwise, which is what the mixer wants to add.
        if ops.intersects(MapperOps::AUDIO) {
            self.apu.add_mapper_output(self.mapper.output());
        }
        self.input.clock();
        self.apu.clock_lazy();
    }
}

impl Bus {
    /// Route a read to whichever component owns `addr`, without moving the console.
    ///
    /// The address decode alone; [`Bus::read`] spends a CPU cycle and then comes through here.
    pub(crate) fn cpu_bus_read(&mut self, addr: u16) -> u8 {
        let addr = match addr {
            0x0800..=0x1FFF => addr & 0x07FF,
            0x2008..=0x3FFF => addr & 0x2007,
            _ => addr,
        };
        self.open_bus = match addr {
            0x0000..=0x07FF => self.patches.read(addr, self.wram[usize::from(addr)]),
            0x4100..=0xFFFF => {
                let val = self
                    .mapper_ops
                    .intersects(MapperOps::SERVES_PRG_READS)
                    .then(|| self.mapper.prg_read(addr))
                    .flatten()
                    .unwrap_or_else(|| self.memory.prg_peek(addr));
                self.patches.read(addr, val)
            }
            0x2002 => self.ppu.read_status(),
            0x2004 => self.ppu.read_oamdata(),
            0x2007 => self.read_data(),
            0x4015 => self.apu.read_status(),
            0x4016 => self.input_read(Player::One),
            0x4017 => self.input_read(Player::Two),
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.ppu.open_bus,
            _ => self.open_bus,
        };
        self.open_bus
    }

    /// Route a read to whichever component owns `addr`, with no side effects at all.
    #[must_use]
    pub(crate) fn cpu_bus_peek(&self, addr: u16) -> u8 {
        let addr = match addr {
            0x0800..=0x1FFF => addr & 0x07FF,
            0x2008..=0x3FFF => addr & 0x2007,
            _ => addr,
        };
        match addr {
            0x0000..=0x07FF => self.patches.read(addr, self.wram[usize::from(addr)]),
            0x4100..=0xFFFF => {
                let val = self
                    .mapper_ops
                    .intersects(MapperOps::SERVES_PRG_READS)
                    .then(|| self.mapper.prg_peek(addr))
                    .flatten()
                    .unwrap_or_else(|| self.memory.prg_peek(addr));
                self.patches.read(addr, val)
            }
            0x2002 => self.ppu.peek_status(),
            0x2004 => self.ppu.peek_oamdata(),
            0x2007 => self.peek_data(),
            0x4015 => self.apu.peek_status(),
            0x4016 => self.input_peek(Player::One),
            0x4017 => self.input_peek(Player::Two),
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.ppu.open_bus,
            _ => self.open_bus,
        }
    }

    /// Route a write to whichever component owns `addr`, without moving the console.
    ///
    /// [`Bus::write`] is the cycle-spending form.
    pub(crate) fn cpu_bus_write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        let addr = match addr {
            0x0800..=0x1FFF => addr & 0x07FF,
            0x2008..=0x3FFF => addr & 0x2007,
            _ => addr,
        };
        match addr {
            0x0000..=0x07FF => self.wram[usize::from(addr)] = val,
            0x4100..=0xFFFF => {
                // Data store first, then let the board act on any register the write hit.
                // Destructured so both fields can be borrowed at once.
                let Self { mapper, memory, .. } = self;
                memory.prg_write(addr, val);
                mapper.write_register(memory, addr, val);
            }
            0x2000 => self.write_ctrl(val),
            0x2001 => self.write_mask(val),
            0x2002 => self.ppu.open_bus = val,
            0x2003 => self.ppu.write_oamaddr(val),
            0x2004 => self.ppu.write_oamdata(val),
            0x2005 => self.ppu.write_scroll(val),
            0x2006 => self.ppu.write_addr(val),
            0x2007 => self.write_data(val),
            0x4000 => self.apu.write_ctrl(Channel::Pulse1, val),
            0x4001 => self.apu.write_sweep(Channel::Pulse1, val),
            0x4002 => self.apu.write_timer_lo(Channel::Pulse1, val),
            0x4003 => self.apu.write_timer_hi(Channel::Pulse1, val),
            0x4004 => self.apu.write_ctrl(Channel::Pulse2, val),
            0x4005 => self.apu.write_sweep(Channel::Pulse2, val),
            0x4006 => self.apu.write_timer_lo(Channel::Pulse2, val),
            0x4007 => self.apu.write_timer_hi(Channel::Pulse2, val),
            0x4008 => self.apu.write_linear_counter(val),
            0x400A => self.apu.write_timer_lo(Channel::Triangle, val),
            0x400B => self.apu.write_timer_hi(Channel::Triangle, val),
            0x400C => self.apu.write_ctrl(Channel::Noise, val),
            0x400E => self.apu.write_timer_lo(Channel::Noise, val),
            0x400F => self.apu.write_length(Channel::Noise, val),
            0x4010 => self.apu.write_timer_lo(Channel::Dmc, val),
            0x4011 => self.apu.write_dmc_output(val),
            0x4012 => self.apu.write_dmc_addr(val),
            0x4013 => self.apu.write_length(Channel::Dmc, val),
            0x4015 => self.apu.write_status(val),
            0x4016 => self.input.write(val),
            0x4017 => self.apu.write_frame_counter(val),
            0x4014 => (), // DMA handled by CPU
            _ => (),
        }
    }
}

impl Bus {
    /// Sets the region, forwarding it to every component.
    pub fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.cpu.set_region(region);
        self.ppu.set_region(region);
        // The board owns the tree's only other region-dependent timing: MMC5's expansion audio
        // clocks its half-frame counter off the CPU clock rate, and its DMC channel off the
        // region rate table, exactly as the APU's does.
        self.mapper.set_region(region);
        self.apu.set_region(region);
        self.input.set_region(region);
        self.clock_sync();
    }

    /// Resets the console, forwarding it to every component.
    ///
    /// A hard reset is a power cycle, so it also re-initialises RAM that a power cycle would not
    /// preserve: the console's work RAM, and the cart's unless a battery is keeping it. The CPU
    /// takes seven cycles to reset, and running them clocks the rest of the console.
    pub fn reset(&mut self, kind: ResetKind) {
        trace!("{kind:?} RESET");

        self.cpu.reset(kind);
        if kind == ResetKind::Hard {
            self.ram_state.fill(&mut **self.wram);
            // Battery-backed cart RAM is left alone: keeping it across a power cycle is what the
            // battery is for, and `.sram` is only written when the cart is unloaded, so wiping it
            // here would throw away a game the player has not put away yet.
            if !self.memory.battery_backed() {
                self.memory.fill_ram(self.ram_state);
            }
        }
        self.ppu.reset(kind);
        self.mapper.reset(kind);
        // Reset can change banking, and the board cannot reach `Memory` from `reset`.
        self.rebuild_mapper_state();
        self.apu.reset(kind);

        // Read straight off the bus so the components are not clocked during the reset itself.
        let lo = self.cpu_bus_read(Cpu::RESET_VECTOR);
        let hi = self.cpu_bus_read(Cpu::RESET_VECTOR + 1);
        self.cpu.pc = u16::from_le_bytes([lo, hi]);

        // The CPU takes 7 cycles to reset/power on
        // See:
        // * <https://www.nesdev.org/wiki/CPU_interrupts>
        // * <http://archive.6502.org/datasheets/synertek_programming_manual.pdf>
        for _ in 0..7 {
            self.start_cycle(self.cpu.start_cycles - 1);
            self.end_cycle(self.cpu.start_cycles + 1);
        }
    }

    /// The cart's whole battery as one slice, with any board-held state brought up to date.
    ///
    /// PRG-RAM first, then whatever the board stages in
    /// [`Src::BatteryExt`](crate::memory::Src::BatteryExt) - so this is what a `.srm` holds, and
    /// for the overwhelming majority of boards it is PRG-RAM exactly, which is what other
    /// emulators write too.
    pub fn sram(&mut self) -> &[u8] {
        let Self { mapper, memory, .. } = self;
        mapper.sync_battery(memory);
        memory.sram()
    }

    /// Writes battery-backed cart RAM to `writer`.
    ///
    /// # Errors
    ///
    /// If the writer fails.
    pub fn save_sram(&mut self, writer: impl Write) -> fs::Result<()> {
        fs::save_sram(writer, self.sram())
    }

    /// Reads battery-backed cart RAM from `reader`.
    ///
    /// A save shorter than the cart's battery - one written before a board grew its
    /// [`Src::BatteryExt`](crate::memory::Src::BatteryExt) - restores what it does hold and leaves
    /// the rest powered on.
    ///
    /// # Errors
    ///
    /// If the reader fails.
    pub fn load_sram(&mut self, reader: impl Read) -> fs::Result<()> {
        let data = fs::load_sram::<Vec<u8>>(reader)?;
        self.set_sram(&data);
        Ok(())
    }

    /// Replaces battery-backed cart RAM with raw bytes, as [`Bus::sram`] hands them out.
    ///
    /// For a frontend that keeps the battery in its own format and hands it back unwrapped; the
    /// `.sram` container is [`Bus::load_sram`].
    pub fn set_sram(&mut self, data: &[u8]) {
        let Self { mapper, memory, .. } = self;
        let sram = memory.sram_mut();
        let len = sram.len().min(data.len());
        sram[..len].copy_from_slice(&data[..len]);
        // Whatever the board keeps outside PRG-RAM has to be told the bytes changed under it.
        mapper.restore_battery(memory);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        apu::noise::ShiftMode,
        input::JoypadBtn,
        mapper::{Cnrom, Nrom},
        memory::Src,
    };

    #[test]
    fn load_cart_values() {
        let mut bus = Bus::default();
        #[rustfmt::skip]
        let rom: [u8; 16] = [
            0x4E, 0x45, 0x53, 0x1A,
            0x00, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];
        let cart = Cart::from_rom("load_cart_test", &mut rom.as_slice(), RamState::default())
            .expect("valid cart");

        let expected_mirroring = cart.mirroring();
        let expected_region = cart.region;
        bus.load_cart(cart);

        assert_eq!(bus.ppu.region, expected_region, "ppu region");
        assert_eq!(bus.apu.region, expected_region, "apu region");
        assert!(
            matches!(bus.mapper, Mapper::Nrom(_)),
            "mapper is Nrom: {:?}",
            bus.mapper
        );
        assert_eq!(bus.mirroring(), expected_mirroring, "mirroring");
    }

    #[test]
    fn load_cart_chr_rom() {
        let mut bus = Bus::default();
        // Cnrom doesn't provide CHR-RAM
        let mut cart = Cart::empty_sized(0x4000, 0x2000);
        cart.mapper = Cnrom::load(&mut cart).unwrap();
        cart.memory.region_mut(Src::Chr).fill(0x66);
        bus.load_cart(cart);

        bus.cpu_bus_write(0x2006, 0x00);
        bus.cpu_bus_write(0x2006, 0x00);
        bus.cpu_bus_read(0x2007);
        assert_eq!(bus.cpu_bus_read(0x2007), 0x66, "chr_rom start");
        bus.cpu_bus_write(0x2006, 0x1F);
        bus.cpu_bus_write(0x2006, 0xFF);
        bus.cpu_bus_read(0x2007);
        assert_eq!(bus.cpu_bus_read(0x2007), 0x66, "chr_rom end");

        // Writes disallowed
        bus.cpu_bus_write(0x2006, 0x00);
        bus.cpu_bus_write(0x2006, 0x10);
        bus.cpu_bus_write(0x2007, 0x77);

        bus.cpu_bus_write(0x2006, 0x00);
        bus.cpu_bus_write(0x2006, 0x10);
        bus.cpu_bus_read(0x2007);
        assert_eq!(bus.cpu_bus_read(0x2007), 0x66, "chr_rom read-only");
    }

    #[test]
    fn load_cart_chr_ram() {
        let mut bus = Bus::default();
        // A zero-sized CHR-ROM yields CHR-RAM.
        let mut cart = Cart::empty_sized(0x4000, 0);
        cart.mapper = Nrom::load(&mut cart).unwrap();
        cart.memory.region_mut(Src::Chr).fill(0x66);
        bus.load_cart(cart);

        bus.cpu_bus_write(0x2006, 0x00);
        bus.cpu_bus_write(0x2006, 0x00);
        bus.cpu_bus_read(0x2007);
        assert_eq!(bus.cpu_bus_read(0x2007), 0x66, "chr_ram start");
        bus.cpu_bus_write(0x2006, 0x1F);
        bus.cpu_bus_write(0x2006, 0xFF);
        bus.cpu_bus_read(0x2007);
        assert_eq!(bus.cpu_bus_read(0x2007), 0x66, "chr_ram end");

        // Writes allowed
        bus.cpu_bus_write(0x2006, 0x10);
        bus.cpu_bus_write(0x2006, 0x00);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.cpu_bus_write(0x2007, 0x77);

        bus.cpu_bus_write(0x2006, 0x10);
        bus.cpu_bus_write(0x2006, 0x00);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.cpu_bus_read(0x2007);
        assert_eq!(bus.cpu_bus_read(0x2007), 0x77, "chr_ram write");
    }

    #[test]
    fn genie_codes() {
        let mut bus = Bus::default();
        let mut cart = Cart::empty_sized(0x8000, 0x2000);

        let code = "YYKPOYZZ"; // The Legend of Zelda: New character with 8 Hearts
        let addr = 0x9F41;
        let orig_value = 0x22; // 3 Hearts
        let new_value = 0x77; // 8 Hearts

        cart.mapper = Nrom::load(&mut cart).unwrap();
        cart.memory.region_mut(Src::PrgRom)[(addr & 0x7FFF) as usize] = orig_value;

        bus.load_cart(cart);
        bus.add_genie_code(GenieCode::new(code.to_string()).expect("valid genie code"));

        assert_eq!(bus.cpu_bus_peek(addr), new_value, "peek code value");
        assert_eq!(bus.cpu_bus_read(addr), new_value, "read code value");
        bus.remove_genie_code(code);
        assert_eq!(bus.cpu_bus_peek(addr), orig_value, "peek orig value");
        assert_eq!(bus.cpu_bus_read(addr), orig_value, "read orig value");
    }

    /// The reason patches are not just Game Genie codes: a frontend's cheat is usually a RAM
    /// address, which the Genie's 15-bit field over `$8000` cannot name.
    #[test]
    fn a_patch_substitutes_wram_reads_the_genie_cannot_reach() {
        let mut bus = Bus::default();
        bus.wram[0x10] = 0x03;
        bus.wram[0x11] = 0x03;

        bus.patches.insert(Patch::new(0x0010, 0x63, None));
        assert_eq!(bus.cpu_bus_read(0x0010), 0x63, "read is substituted");
        assert_eq!(bus.cpu_bus_peek(0x0010), 0x63, "and so is peek");
        assert_eq!(bus.cpu_bus_peek(0x0011), 0x03, "the next byte is not");
        // The mirrors fold onto the same address, so they are patched with it.
        assert_eq!(bus.cpu_bus_peek(0x0810), 0x63, "and its mirror");

        // Substitution is on the way out, so the game's own writes still land - which is what
        // makes a cheat hold rather than flicker the way a once-a-frame poke would.
        bus.cpu_bus_write(0x0010, 0x01);
        assert_eq!(bus.wram[0x10], 0x01, "the write reaches memory");
        assert_eq!(
            bus.cpu_bus_peek(0x0010),
            0x63,
            "and the read still does not"
        );

        bus.patches.remove(0x0010);
        assert_eq!(bus.cpu_bus_peek(0x0010), 0x01);
    }

    #[test]
    fn clock() {
        let mut bus = Bus::default();

        bus.ppu_clock_to(12);
        assert_eq!(bus.ppu.master_clock, 12, "ppu clock");
        bus.cpu_clock();
        assert_eq!(bus.apu.master_clock, 1, "apu clock");
    }

    #[test]
    fn read_write_ram() {
        let mut bus = Bus::default();

        bus.cpu_bus_write(0x0001, 0x66);
        assert_eq!(bus.cpu_bus_peek(0x0001), 0x66, "peek ram");
        assert_eq!(bus.cpu_bus_read(0x0001), 0x66, "read ram");
        assert_eq!(bus.cpu_bus_read(0x0801), 0x66, "peek mirror 1");
        assert_eq!(bus.cpu_bus_read(0x0801), 0x66, "read mirror 1");
        assert_eq!(bus.cpu_bus_read(0x1001), 0x66, "peek mirror 2");
        assert_eq!(bus.cpu_bus_read(0x1001), 0x66, "read mirror 2");
        assert_eq!(bus.cpu_bus_read(0x1801), 0x66, "peek mirror 3");
        assert_eq!(bus.cpu_bus_read(0x1801), 0x66, "read mirror 3");

        bus.cpu_bus_write(0x0802, 0x77);
        assert_eq!(bus.cpu_bus_read(0x0002), 0x77, "write mirror 1");
        bus.cpu_bus_write(0x1002, 0x88);
        assert_eq!(bus.cpu_bus_read(0x0002), 0x88, "write mirror 2");
        bus.cpu_bus_write(0x1802, 0x99);
        assert_eq!(bus.cpu_bus_read(0x0002), 0x99, "write mirror 3");
    }

    /// $2000-$2007 repeat every 8 bytes up to $3FFF, so the mirror mask is what decides which
    /// register a write lands on. Reads of the write-only registers return the PPU's open bus
    /// rather than anything latched.
    #[test]
    fn read_write_ppu() {
        let mut bus = Bus::default();

        bus.cpu_bus_write(0x2000, 0x80);
        assert!(bus.ppu.ctrl_nmi_enabled, "$2000 PPUCTRL");
        bus.cpu_bus_write(0x3FF8, 0x00);
        assert!(!bus.ppu.ctrl_nmi_enabled, "$3FF8 mirrors $2000");

        bus.cpu_bus_write(0x2001, 0x1E);
        assert_eq!(bus.ppu.mask_bits.bits(), 0x1E, "$2001 PPUMASK");
        bus.cpu_bus_write(0x3FF9, 0x00);
        assert_eq!(bus.ppu.mask_bits.bits(), 0x00, "$3FF9 mirrors $2001");
        bus.cpu_bus_write(0x2003, 0x42);
        assert_eq!(bus.ppu.oamaddr, 0x42, "$2003 OAMADDR");

        // OAMDATA round-trips through $2004, and the address post-increments on write.
        bus.cpu_bus_write(0x2003, 0x10);
        bus.cpu_bus_write(0x2004, 0x99);
        assert_eq!(bus.ppu.oamaddr, 0x11, "OAMADDR increments on write");
        bus.cpu_bus_write(0x2003, 0x10);
        assert_eq!(bus.cpu_bus_read(0x2004), 0x99, "$2004 OAMDATA");

        // The write-only registers read back the PPU's open bus, not their contents.
        bus.ppu.open_bus = 0xA5;
        for addr in [0x2000, 0x2001, 0x2003, 0x2005, 0x2006] {
            assert_eq!(bus.cpu_bus_read(addr), 0xA5, "${addr:04X} is write-only");
        }

        // $2002 clears the vblank flag as a side effect, so a second read differs - and `peek`
        // must not do it.
        bus.ppu.set_in_vblank(true);
        assert_ne!(bus.cpu_bus_peek(0x2002) & 0x80, 0, "peek sees vblank");
        assert_ne!(bus.cpu_bus_peek(0x2002) & 0x80, 0, "and leaves it set");
        assert_ne!(bus.cpu_bus_read(0x2002) & 0x80, 0, "read sees vblank");
        assert_eq!(bus.cpu_bus_read(0x2002) & 0x80, 0, "and clears it");
    }

    /// $4015 is the APU status register both ways, but $4017 is not symmetric: writing it sets the
    /// APU frame counter while reading it returns controller two. Getting that backwards is silent.
    #[test]
    fn read_write_apu() {
        let mut bus = Bus::default();

        // $4015 write enables length counters; reading it reports which are non-zero.
        bus.cpu_bus_write(0x4015, 0x0F);
        assert!(bus.apu.pulse1.length.enabled, "pulse1 enabled");
        assert!(bus.apu.pulse2.length.enabled, "pulse2 enabled");
        assert!(bus.apu.triangle.length.enabled, "triangle enabled");
        assert!(bus.apu.noise.length.enabled, "noise enabled");
        bus.cpu_bus_write(0x4015, 0x00);
        assert!(!bus.apu.pulse1.length.enabled, "pulse1 disabled");

        // $4017 write is the frame counter; bit 6 inhibits the frame IRQ.
        bus.cpu_bus_write(0x4017, 0x40);
        assert!(
            bus.apu.frame_counter.inhibit_irq,
            "$4017 bit 6 inhibits the frame IRQ"
        );

        // $4017 read is controller two, not the frame counter.
        bus.input.joypads[1].set_button(JoypadBtn::A, true);
        bus.cpu_bus_write(0x4016, 0x01);
        bus.cpu_bus_write(0x4016, 0x00);
        assert_eq!(
            bus.cpu_bus_read(0x4017) & 0x01,
            0x01,
            "$4017 reads controller two"
        );
    }

    /// $4000-$4003 is pulse 1 and $4004-$4007 is pulse 2. The two blocks must stay independent.
    #[test]
    fn write_apu_pulse() {
        let mut bus = Bus::default();

        bus.cpu_bus_write(0x4000, 0x3F); // duty 0, constant volume 15
        bus.cpu_bus_write(0x4002, 0x34); // timer low
        bus.cpu_bus_write(0x4003, 0x01); // timer high
        assert_eq!(bus.apu.pulse1.real_period, 0x134, "pulse1 period");
        assert_eq!(bus.apu.pulse2.real_period, 0, "pulse2 untouched");

        bus.cpu_bus_write(0x4006, 0x78);
        bus.cpu_bus_write(0x4007, 0x02);
        assert_eq!(bus.apu.pulse2.real_period, 0x278, "pulse2 period");
        assert_eq!(bus.apu.pulse1.real_period, 0x134, "pulse1 still untouched");

        // $4001/$4005 are the sweep units.
        bus.cpu_bus_write(0x4001, 0x8F);
        assert!(bus.apu.pulse1.sweep.enabled, "$4001 pulse1 sweep");
        assert!(!bus.apu.pulse2.sweep.enabled, "pulse2 sweep untouched");
        bus.cpu_bus_write(0x4005, 0x8F);
        assert!(bus.apu.pulse2.sweep.enabled, "$4005 pulse2 sweep");
    }

    /// $4008/$400A/$400B is the triangle. $4009 is unmapped and must do nothing.
    #[test]
    fn write_apu_triangle() {
        let mut bus = Bus::default();

        bus.cpu_bus_write(0x400A, 0x56);
        bus.cpu_bus_write(0x400B, 0x03);
        assert_eq!(bus.apu.triangle.timer.period, 0x356, "triangle period");

        bus.cpu_bus_write(0x4008, 0x7F);
        assert_eq!(
            bus.apu.triangle.linear.counter_reload, 0x7F,
            "$4008 linear counter"
        );

        // $4009 is not a register; it must not disturb the channel.
        let before = bus.apu.triangle.timer.period;
        bus.cpu_bus_write(0x4009, 0xFF);
        assert_eq!(bus.apu.triangle.timer.period, before, "$4009 is unmapped");
    }

    /// $400C/$400E/$400F is the noise channel. $400D is unmapped.
    #[test]
    fn write_apu_noise() {
        let mut bus = Bus::default();

        bus.cpu_bus_write(0x400E, 0x80 | 0x04);
        assert_eq!(bus.apu.noise.shift_mode, ShiftMode::One, "$400E shift mode");

        bus.cpu_bus_write(0x400C, 0x3F);
        assert!(bus.apu.noise.envelope.constant_volume, "$400C envelope");

        // The length counter latches a reload value here; the frame counter loads it later.
        bus.cpu_bus_write(0x4015, 0x08); // enable, or the write is ignored entirely
        bus.cpu_bus_write(0x400F, 0x08);
        assert_ne!(bus.apu.noise.length.reload, 0, "$400F length reload");
        bus.cpu_bus_write(0x4015, 0x00);
        bus.cpu_bus_write(0x400F, 0x10);
        assert_eq!(
            bus.apu.noise.length.reload, 254,
            "a disabled channel ignores the write"
        );

        let before = bus.apu.noise.timer.period;
        bus.cpu_bus_write(0x400D, 0xFF);
        assert_eq!(bus.apu.noise.timer.period, before, "$400D is unmapped");
    }

    /// $4010-$4013 is the DMC. Sample address and length are stored scaled, not raw.
    #[test]
    fn write_dmc() {
        let mut bus = Bus::default();

        bus.cpu_bus_write(0x4010, 0x0F); // rate index 15, IRQ and loop clear
        assert!(!bus.apu.dmc.irq_enabled, "$4010 IRQ disabled");
        assert!(!bus.apu.dmc.loops, "$4010 loop clear");
        bus.cpu_bus_write(0x4010, 0xC0);
        assert!(bus.apu.dmc.irq_enabled, "$4010 bit 7 enables the IRQ");
        assert!(bus.apu.dmc.loops, "$4010 bit 6 sets loop");

        bus.cpu_bus_write(0x4011, 0xFF);
        assert_eq!(bus.apu.dmc.output_level, 0x7F, "$4011 keeps 7 bits");

        bus.cpu_bus_write(0x4012, 0x02);
        assert_eq!(bus.apu.dmc.sample_addr, 0xC080, "$4012 is $C000 + n*64");

        bus.cpu_bus_write(0x4013, 0x02);
        assert_eq!(bus.apu.dmc.sample_length, 0x21, "$4013 is n*16 + 1");
    }

    /// $4016 writes strobe both controllers; $4016 and $4017 read them back one bit at a time.
    #[test]
    fn read_write_input() {
        let mut bus = Bus::default();

        bus.input.joypads[0].set_button(JoypadBtn::A, true);
        bus.input.joypads[0].set_button(JoypadBtn::Right, true);

        // Strobe high then low latches the button state and rewinds to bit 0.
        bus.cpu_bus_write(0x4016, 0x01);
        bus.cpu_bus_write(0x4016, 0x00);

        // A, B, Select, Start, Up, Down, Left, Right - so bit 0 is A and bit 7 is Right.
        let bits: Vec<u8> = (0..8).map(|_| bus.cpu_bus_read(0x4016) & 0x01).collect();
        assert_eq!(bits, [1, 0, 0, 0, 0, 0, 0, 1], "controller one shifts out");

        // Re-strobing rewinds it.
        bus.cpu_bus_write(0x4016, 0x01);
        bus.cpu_bus_write(0x4016, 0x00);
        assert_eq!(
            bus.cpu_bus_read(0x4016) & 0x01,
            0x01,
            "back to the A button"
        );

        // Controller two is a separate shift register on $4017.
        assert_eq!(
            bus.cpu_bus_read(0x4017) & 0x01,
            0x00,
            "controller two is idle"
        );
    }

    /// Everything from $4100 up is the cartridge: the write goes to memory first and then to the
    /// board, and reads come back through the page table.
    #[test]
    fn read_write_mapper() {
        let mut bus = Bus::default();
        // 16K of CHR-ROM, i.e. two 8K banks, so a bank switch is visible.
        let mut cart = Cart::empty_sized(0x8000, 0x4000);
        cart.mapper = Cnrom::load(&mut cart).expect("valid mapper");
        cart.memory.region_mut(Src::Chr).fill(0x11);
        cart.memory.region_mut(Src::Chr)[0x2000..].fill(0x22);
        bus.load_cart(cart);

        let read_chr = |bus: &mut Bus| {
            bus.cpu_bus_write(0x2006, 0x00);
            bus.cpu_bus_write(0x2006, 0x00);
            bus.cpu_bus_read(0x2007); // discard the buffered read
            bus.cpu_bus_read(0x2007)
        };
        assert_eq!(read_chr(&mut bus), 0x11, "CHR bank 0");

        // CNROM takes its bank from any write to $8000-$FFFF.
        bus.cpu_bus_write(0x8000, 0x01);
        assert_eq!(read_chr(&mut bus), 0x22, "the write reached the board");

        // Below $4100 is not the cartridge, so it must not reach the board.
        bus.cpu_bus_write(0x4000, 0x00);
        assert_eq!(read_chr(&mut bus), 0x22, "$4000 is the APU, not the mapper");
    }

    /// A hard reset re-fills WRAM from the configured RAM state; a soft reset leaves it alone.
    #[test]
    fn reset() {
        let mut bus = Bus {
            ram_state: RamState::AllZeros,
            ..Default::default()
        };

        bus.cpu_bus_write(0x0001, 0x66);
        bus.cpu_bus_write(0x2000, 0x80);

        bus.reset(ResetKind::Soft);
        assert_eq!(
            bus.cpu_bus_peek(0x0001),
            0x66,
            "a soft reset preserves WRAM"
        );

        bus.reset(ResetKind::Hard);
        assert_eq!(bus.cpu_bus_peek(0x0001), 0x00, "a hard reset clears WRAM");
    }

    /// A hard reset is a power cycle, so cart RAM comes back up in whatever state the console is
    /// configured for - unless a battery is keeping it, which is the whole point of the battery.
    #[test]
    fn hard_reset_refills_cart_ram_unless_it_is_battery_backed() {
        for battery_backed in [false, true] {
            let mut bus = Bus {
                ram_state: RamState::AllOnes,
                ..Default::default()
            };
            let mut cart = Cart::empty_sized(0x4000, 0x2000);
            cart.mapper = Nrom::load(&mut cart).expect("valid mapper");
            bus.load_cart(cart);
            bus.memory.set_battery_backed(battery_backed);
            bus.memory.region_mut(Src::PrgRam).fill(0x42);

            bus.reset(ResetKind::Soft);
            assert!(
                bus.memory
                    .region_ref(Src::PrgRam)
                    .iter()
                    .all(|&b| b == 0x42),
                "a soft reset leaves cart RAM alone"
            );

            bus.reset(ResetKind::Hard);
            let expected = if battery_backed { 0x42 } else { 0xFF };
            assert!(
                bus.memory
                    .region_ref(Src::PrgRam)
                    .iter()
                    .all(|&b| b == expected),
                "battery_backed {battery_backed}: cart RAM after a power cycle"
            );
        }
    }
}
