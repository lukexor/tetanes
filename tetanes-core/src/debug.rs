//! Debugger hooks.
//!
//! A debugger is a callback plus the condition that fires it. The callback is handed the whole
//! [`Bus`](crate::bus::Bus) - so it can take whatever state snapshot it needs at that point during
//! emulation.

use crate::{bus::Bus, memory::Memory};
use bitflags::bitflags;
use std::sync::Arc;

pub mod expr;

/// A ring buffer of the program counters most recently executed.
///
/// Executed instructions have to be recorded as the console runs: 6502 instructions are one to
/// three bytes with no alignment, so the stream before an address cannot be recovered by decoding
/// backwards without a known address to disassemble from. Recording them lets a debugger show the
/// instructions leading up to where it stopped execution.
#[derive(Debug, Clone)]
#[must_use]
pub struct PcHistory {
    entries: Box<[u16]>,
    /// Where the next push goes, wrapping at `entries.len()`.
    head: usize,
    /// How many entries have been filled, saturating at `entries.len()`.
    len: usize,
}

impl PcHistory {
    /// Create a history holding the last `capacity` program counters.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: vec![0; capacity.max(1)].into_boxed_slice(),
            head: 0,
            len: 0,
        }
    }

    /// Record an executed program counter, dropping the oldest once full.
    #[inline]
    pub fn push(&mut self, pc: u16) {
        self.entries[self.head] = pc;
        self.head = (self.head + 1) % self.entries.len();
        self.len = (self.len + 1).min(self.entries.len());
    }

    /// The recorded program counters, oldest first, which is the order they are read in.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = u16> + '_ {
        // `head` is one past the newest, so the oldest live entry is `len` behind it.
        let start = (self.head + self.entries.len() - self.len) % self.entries.len();
        (0..self.len).map(move |i| self.entries[(start + i) % self.entries.len()])
    }

    /// How many program counters are recorded.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been recorded yet.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Forget what was recorded, keeping the capacity.
    pub const fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

/// Why a call frame was pushed.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum FrameKind {
    /// A `JSR`.
    Call,
    /// The NMI vector, taken when the PPU asserts it.
    Nmi,
    /// The IRQ vector, taken when the board or the APU pulls the line low.
    Irq,
    /// A `BRK`, which reaches the IRQ vector by executing rather than by a line going low.
    Brk,
}

/// One call execution is currently inside.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub struct CallFrame {
    /// The `JSR`, or the instruction an interrupt arrived before.
    pub caller: u16,
    /// The subroutine, or the handler the vector named.
    pub entry: u16,
    /// The stack pointer once the return address was pushed. The frame is live while the stack
    /// pointer is no higher than this.
    pub sp: u8,
    /// Why the frame was pushed.
    pub kind: FrameKind,
}

/// The calls execution is currently inside, outermost first.
///
/// [`PcHistory`] says what has run, this says how the console reached where it is. A frame is
/// pushed by a `JSR` and by an interrupt taking a vector, and dropped by watching the stack
/// pointer rather than by watching for an `RTS`: a return address is often discarded (`PLA`,
/// `PLA`) or reached by a jump, and the stack pointer catches all of those the same way.
#[derive(Debug, Default, Clone)]
#[must_use]
pub struct CallStack {
    frames: Vec<CallFrame>,
}

impl CallStack {
    /// How many frames the stack records. A frame takes at least the two bytes a `JSR` pushes,
    /// out of a 256 byte stack page.
    const MAX_DEPTH: usize = 128;

    /// Create an empty call stack.
    pub fn new() -> Self {
        Self {
            frames: Vec::with_capacity(16),
        }
    }

    /// Drop the frames the stack pointer has risen past. An `RTS` raises it back over the frame
    /// it returns from.
    #[inline]
    pub fn unwind_to(&mut self, sp: u8) {
        while self.frames.last().is_some_and(|frame| sp > frame.sp) {
            self.frames.pop();
        }
    }

    /// Record a call.
    ///
    /// Each frame sits below the one before it, so the depth is bounded by the stack page and
    /// only a stack pointer that wrapped can reach the limit.
    pub fn push(&mut self, frame: CallFrame) {
        if self.frames.len() < Self::MAX_DEPTH {
            self.frames.push(frame);
        }
    }

    /// The frames execution is inside, outermost first.
    pub fn frames(&self) -> &[CallFrame] {
        &self.frames
    }

    /// How deep the stack is.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the stack has no frames.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Forget the recorded frames, keeping the capacity.
    pub fn clear(&mut self) {
        self.frames.clear();
    }
}

bitflags! {
    /// What execution has shown a byte to be.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct ByteKind: u8 {
        /// Ran as the first byte of an instruction.
        const CODE = 1;
        /// A `JSR` destination or subroutine entry instruction.
        const SUB_ENTRY = 1 << 1;
    }
}

/// What execution has revealed about each byte of cartridge memory.
///
/// Only execution writes here, nothing decodes ahead of it. A byte is only code if it has been
/// executed, which the disassembler cannot work out on its own: 6502 instructions are one to three
/// bytes with no alignment, so a decode that starts inside data stays out of step with the real
/// instruction boundaries until it happens to realign.
///
/// Keyed by [`Memory`] offset instead of CPU address, so a mark survives bank switches to another
/// address. Two banks that share an address do not share marks. See [`Memory::prg_offset`].
#[derive(Debug, Clone)]
#[must_use]
pub struct CodeMap {
    kinds: Box<[ByteKind]>,
    generation: u64,
    /// Which cart the offsets address. See [`CodeMap::covers`].
    rom_crc32: u32,
}

impl CodeMap {
    /// A map covering `len` bytes of the cart with CRC `rom_crc32`, with nothing yet known about
    /// any of them.
    pub fn new(len: usize, rom_crc32: u32) -> Self {
        Self {
            kinds: vec![ByteKind::empty(); len].into_boxed_slice(),
            generation: 0,
            rom_crc32,
        }
    }

    /// Mark what the byte at `offset` has been shown to be.
    ///
    /// An offset past the end of the memory this was built for is ignored, so a caller can pass
    /// on whatever a page table gave it without checking.
    #[inline]
    pub fn mark(&mut self, offset: usize, kind: ByteKind) {
        if let Some(entry) = self.kinds.get_mut(offset)
            && !entry.contains(kind)
        {
            *entry |= kind;
            self.generation += 1;
        }
    }

    /// What is known about the byte at `offset`, empty for one nothing has revealed yet.
    pub fn kind(&self, offset: usize) -> ByteKind {
        self.kinds
            .get(offset)
            .copied()
            .unwrap_or_else(ByteKind::empty)
    }

    /// Whether the byte at `offset` has run as the first byte of an instruction.
    pub fn is_code(&self, offset: usize) -> bool {
        self.kind(offset).contains(ByteKind::CODE)
    }

    /// Bumped whenever mark records something new.
    ///
    /// A view built from the map becomes stale as code executes. Comparing this is how a consumer
    /// knows to rebuild.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether this was recorded against `memory`.
    ///
    /// An offset means nothing against another arena, so a map kept across a cart change has to be
    /// refused rather than reused. The CRC says which game and the length says the geometry - two
    /// dumps of one game can differ in trainer or padding, and an arena built with no ROM to hash
    /// has only the geometry to go on.
    pub fn covers(&self, memory: &Memory) -> bool {
        self.rom_crc32 == memory.rom_crc32() && self.kinds.len() == memory.len()
    }
}

bitflags! {
    /// The ways an address can be touched.
    ///
    /// A breakpoint has the set it stops on, so one range covers reads, writes and execution in
    /// whatever combination the user ticked.
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
    #[must_use]
    pub struct Access: u8 {
        /// Fetched as an instruction.
        ///
        /// Checked between instructions, since a fetch and an operand read look alike on the bus.
        /// The console stops *before* running the instruction, where a read or a write stops
        /// after it.
        ///
        /// The two halves are served in different places. A breakpoint that records is offered
        /// each instruction by `Bus::check_exec`. One that stops belongs to the driver, through
        /// [`ControlDeck::clock_frame_until`](crate::control_deck::ControlDeck::clock_frame_until),
        /// so that a stop unwinds to whoever is clocking rather than landing mid-instruction.
        const EXEC = 1;
        /// Read by an instruction, other than the fetch.
        ///
        /// A DMA moves its bytes without an instruction asking, through `Bus::cpu_bus_read`, so
        /// the 256 reads an OAM transfer makes are not reported.
        const READ = 1 << 1;
        /// Written by an instruction.
        ///
        /// The write half of a DMA is likewise unreported: `$2004` takes 256 bytes during an OAM
        /// transfer without a `STA` behind any of them.
        const WRITE = 1 << 2;
    }
}

/// A range of CPU addresses the console stops on, or records, when one is accessed.
///
/// `offset` pins it to the bytes it was set over, and `condition` narrows it to the accesses that
/// satisfy an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Breakpoint {
    /// First address covered.
    pub start: u16,
    /// Last address covered, inclusive. Equal to `start` for a single address.
    pub end: u16,
    /// Where `start` sat in the [`Memory`] arena when the breakpoint was set.
    ///
    /// A CPU address names a window, not a byte: the instruction at `$8123` in one bank and the
    /// one at `$8123` in another are different code that a breakpoint on the address cannot tell
    /// apart. Recording the offset pins the breakpoint to the bytes it was set over, the way
    /// [`CodeMap`] pins a mark. `None` where the address has no offset to record - work RAM, the
    /// registers, an unmapped page - which leaves those keyed by address.
    pub offset: Option<u32>,
    /// Which accesses trip it.
    pub access: Access,
    /// Cleared to record the access and let the console run on.
    pub breaks: bool,
    /// An expression that has to hold as well, or `None` to trip on every covered access.
    ///
    /// Evaluated at the access, against the console as it stands part way through the
    /// instruction. Parsing happens when the breakpoint is set, so what is asked here is a
    /// compiled form. See [`expr`].
    pub condition: Option<expr::Expr>,
}

impl Breakpoint {
    /// Whether this covers `addr` and stops on `access`.
    #[inline]
    pub const fn covers(&self, addr: u16, access: Access) -> bool {
        self.start <= addr && addr <= self.end && self.access.contains(access)
    }

    /// Whether this covers `addr` and stops on `access`, with the bank it was set in still mapped
    /// at `start`.
    ///
    /// The whole range answers to `start`'s bank rather than each address answering to its own.
    /// The arena is contiguous across a range only while one bank holds all of it, so resolving
    /// per address would leave a range typed across two windows covering its first half from the
    /// moment it was set.
    #[inline]
    pub fn matches(&self, memory: &Memory, addr: u16, access: Access) -> bool {
        self.covers(addr, access)
            && match self.offset {
                Some(offset) => memory.prg_offset(self.start) == Some(offset as usize),
                None => true,
            }
    }

    /// Whether this trips on `access` to `addr`, condition and all.
    ///
    /// The condition is asked last, since it is the only part that can read the whole console.
    #[inline]
    pub fn fires(&self, bus: &Bus, addr: u16, access: Access) -> bool {
        self.matches(&bus.memory, addr, access)
            && self
                .condition
                .as_ref()
                .is_none_or(|condition| condition.is_true(bus))
    }
}

/// What the armed breakpoints make of one access.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub struct Verdict {
    /// A breakpoint that stops the console tripped.
    pub stop: bool,
    /// A breakpoint that keeps the console running tripped, so the access is worth logging.
    pub record: bool,
}

/// An access a [`Breakpoint`] caught.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub struct AccessHit {
    /// The instruction that made the access.
    pub pc: u16,
    /// The address touched.
    pub addr: u16,
    /// How it was touched, which is one of [`Access`]'s flags rather than a set.
    pub access: Access,
    /// The byte read or written.
    pub value: u8,
}

/// The armed breakpoints, with a bitmap over every address they cover.
///
/// The bitmap is a pre-filter: a breakpoint has a range, and later a condition, that no bitmap
/// expresses. An access matching none of them tests one bit and stops there, which keeps this
/// affordable on [`Bus::read`](crate::bus::Bus) and `Bus::write`.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct Breakpoints {
    /// One bit per CPU address, set where any breakpoint covers it, whatever the access.
    covered: Box<[u64; Self::WORDS]>,
    /// The armed breakpoints, scanned only once the bitmap says an address is covered.
    list: Vec<Breakpoint>,
    /// Accesses the breakpoints that keep running have caught, waiting to be drained.
    hits: Vec<AccessHit>,
}

impl Breakpoints {
    /// 64K addresses, one bit each.
    const WORDS: usize = (u16::MAX as usize + 1) / 64;
    /// How many breakpoints are kept, which bounds the scan a hit pays for.
    pub const MAX: usize = 64;
    /// How many recorded accesses are kept between drains, so a breakpoint on a hot address
    /// cannot grow without limit.
    const MAX_HITS: usize = 256;

    /// Arm `breakpoints`, dropping any past [`Breakpoints::MAX`].
    pub fn new(breakpoints: impl IntoIterator<Item = Breakpoint>) -> Self {
        let mut covered = Box::new([0u64; Self::WORDS]);
        let list = breakpoints.into_iter().take(Self::MAX).collect::<Vec<_>>();
        for breakpoint in &list {
            for addr in breakpoint.start..=breakpoint.end {
                let addr = usize::from(addr);
                covered[addr / 64] |= 1 << (addr % 64);
            }
        }
        Self {
            covered,
            list,
            hits: Vec::new(),
        }
    }

    /// Whether any breakpoint covers `addr`, whatever the access.
    ///
    /// One bit test, so asking per access, and per instruction, stays affordable.
    #[inline]
    pub fn watches(&self, addr: u16) -> bool {
        let addr = usize::from(addr);
        self.covered[addr / 64] & (1 << (addr % 64)) != 0
    }

    /// What the armed breakpoints make of `access` to `addr`.
    ///
    /// An empty verdict for the addresses nothing watches, which is the answer almost every time.
    /// `bus` is read only past the bitmap, so an unwatched address pays for the bit and nothing
    /// else.
    ///
    /// Deciding is separate from [`Breakpoints::record`] because a condition reads the whole
    /// console, and the console owns the breakpoints. Asking with a shared borrow and recording
    /// with a mutable one keeps both without handing the breakpoints out.
    pub fn check(&self, bus: &Bus, addr: u16, access: Access) -> Verdict {
        let mut verdict = Verdict::default();
        if !self.watches(addr) {
            return verdict;
        }
        for breakpoint in &self.list {
            if breakpoint.fires(bus, addr, access) {
                if breakpoint.breaks {
                    verdict.stop = true;
                } else {
                    verdict.record = true;
                }
            }
        }
        verdict
    }

    /// Log an access a breakpoint that records caught, dropping it once the log is full.
    pub fn record(&mut self, hit: AccessHit) {
        if self.hits.len() < Self::MAX_HITS {
            self.hits.push(hit);
        }
    }

    /// Take over what `previous` caught, for a set replacing it.
    ///
    /// A caught access is a record of what the console did, so it outlives the breakpoints that
    /// were armed when it happened.
    pub fn adopt_hits(&mut self, previous: &mut Self) {
        self.hits = std::mem::take(&mut previous.hits);
        self.hits.truncate(Self::MAX_HITS);
    }

    /// Take the accesses recorded since the last drain.
    pub fn drain_hits(&mut self) -> Vec<AccessHit> {
        std::mem::take(&mut self.hits)
    }

    /// Put back what [`Breakpoints::drain_hits`] took, dropping anything caught in between.
    ///
    /// Run-ahead drains before it speculates and restores once it has rewound, so the accesses
    /// its discarded frames caught are discarded with them instead of being reported twice.
    pub fn restore_hits(&mut self, hits: Vec<AccessHit>) {
        self.hits = hits;
    }

    /// Whether nothing is armed, in which case the console keeps the unwatched path.
    pub const fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// Runs a callback once the PPU reaches a given dot.
#[derive(Clone)]
#[must_use]
pub struct Debugger {
    /// The cycle within `scanline`.
    pub cycle: u16,
    /// The scanline.
    pub scanline: u16,
    /// What to run when the cycle/scanline is reached.
    pub callback: Arc<dyn Fn(&Bus) + Send + Sync + 'static>,
}

impl Default for Debugger {
    fn default() -> Self {
        Self {
            cycle: u16::MAX,
            scanline: u16::MAX,
            callback: Arc::new(|_| {}),
        }
    }
}

impl PartialEq for Debugger {
    fn eq(&self, other: &Self) -> bool {
        self.cycle == other.cycle && self.scanline == other.scanline
    }
}

impl Bus {
    /// Set (`Some`) or clear (`None`) a debugger callback.
    #[inline]
    pub fn set_debugger(&mut self, debugger: Option<Debugger>) {
        self.debugger_active = debugger.is_some();
        self.debugger = debugger.unwrap_or_default();
    }

    /// Start recording what executes into a [`CodeMap`].
    ///
    /// Resumes `code_map` when it was built for the loaded cart, otherwise starts a fresh one.
    /// Pass `None` to start fresh regardless.
    pub fn attach_code_map(&mut self, code_map: Option<CodeMap>) {
        self.code_map = Some(match code_map {
            Some(code_map) if code_map.covers(&self.memory) => code_map,
            _ => CodeMap::new(self.memory.len(), self.memory.rom_crc32()),
        });
    }

    /// Stop recording and hand back what was recorded, for a later [`Bus::attach_code_map`].
    ///
    /// The marks stay valid for as long as the cart is loaded. Only execution adds to them, so a
    /// map that misses what ran while it was detached reads as unmarked rather than as wrong.
    pub const fn detach_code_map(&mut self) -> Option<CodeMap> {
        self.code_map.take()
    }
}

impl std::fmt::Debug for Debugger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Debugger")
            .field("cycle", &self.cycle)
            .field("scanline", &self.scanline)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Access, AccessHit, Breakpoint, Breakpoints, ByteKind, CallStack, CodeMap, FrameKind,
        PcHistory, expr::Expr,
    };
    use crate::{
        bus::Bus, cart::Cart, common::ResetKind, control_deck::ControlDeck, cpu::Cpu, mapper::Nrom,
        memory::Src,
    };

    /// One 16 KiB PRG bank, which is the window the banked tests swap.
    const BANK: usize = 0x4000;

    #[test]
    fn an_unused_history_reads_as_empty() {
        let history = PcHistory::new(4);
        assert!(history.is_empty());
        assert_eq!(history.iter().count(), 0);
    }

    #[test]
    fn a_partly_filled_history_reads_oldest_first() {
        let mut history = PcHistory::new(4);
        for pc in [0xC000, 0xC003, 0xC005] {
            history.push(pc);
        }
        assert_eq!(history.len(), 3);
        assert_eq!(history.iter().collect::<Vec<_>>(), [0xC000, 0xC003, 0xC005]);
    }

    #[test]
    fn a_wrapped_history_keeps_the_most_recent_oldest_first() {
        let mut history = PcHistory::new(3);
        for pc in [0x8000, 0x8001, 0x8002, 0x8003, 0x8004] {
            history.push(pc);
        }
        // Capacity, not push count: the two oldest have been dropped.
        assert_eq!(history.len(), 3);
        assert_eq!(history.iter().collect::<Vec<_>>(), [0x8002, 0x8003, 0x8004]);
    }

    #[test]
    fn clearing_keeps_the_capacity_and_starts_over() {
        let mut history = PcHistory::new(2);
        history.push(0xE000);
        history.clear();
        assert!(history.is_empty());
        history.push(0xE001);
        assert_eq!(history.iter().collect::<Vec<_>>(), [0xE001]);
    }

    /// A console running `program` from `$0700`, recording the calls it makes.
    fn running(program: &[u8]) -> Bus {
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        cart.mapper = Nrom::load(&mut cart).expect("mapper");
        bus.load_cart(cart);
        bus.reset(ResetKind::Hard);
        for (offset, byte) in program.iter().enumerate() {
            bus.cpu_bus_write(0x0700 + offset as u16, *byte);
        }
        bus.cpu.pc = 0x0700;
        bus.call_stack = Some(CallStack::new());
        bus
    }

    #[test]
    fn a_call_is_recorded_and_its_return_drops_it() {
        // JSR $0710, landing on an RTS.
        let mut bus = running(&[Cpu::JSR, 0x10, 0x07]);
        bus.cpu_bus_write(0x0710, Cpu::RTS);

        bus.clock_instr();
        let frames = bus.call_stack.as_ref().expect("recording").frames();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].caller, 0x0700);
        assert_eq!(frames[0].entry, 0x0710);
        assert_eq!(frames[0].kind, FrameKind::Call);

        bus.clock_instr();
        assert!(bus.call_stack.expect("recording").is_empty());
    }

    /// The case that watching for an `RTS` would miss: a subroutine that pulls its own return
    /// address off and jumps elsewhere. Half a return address is already unusable, so the frame
    /// goes with the first `PLA`.
    #[test]
    fn discarding_a_return_address_drops_the_frame() {
        // JSR $0710, landing on PLA, PLA.
        let mut bus = running(&[Cpu::JSR, 0x10, 0x07]);
        for (addr, byte) in [(0x0710u16, 0x68u8), (0x0711, 0x68)] {
            bus.cpu_bus_write(addr, byte);
        }

        bus.clock_instr();
        assert_eq!(bus.call_stack.as_ref().expect("recording").len(), 1);
        bus.clock_instr();
        bus.clock_instr();
        assert!(bus.call_stack.expect("recording").is_empty());
    }

    /// Each frame sits below the one that called it, which is the order the stack pointer
    /// unwinds them in.
    #[test]
    fn frames_descend_from_the_outermost_call() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("test_roms/spritecans.nes")
            .expect("load rom");
        deck.set_call_stack(true);

        let mut deepest = 0;
        for _ in 0..100_000 {
            deck.clock_instr().expect("clock instruction");
            let frames = deck.call_stack().expect("recording").frames();
            deepest = deepest.max(frames.len());
            for pair in frames.windows(2) {
                assert!(
                    pair[1].sp < pair[0].sp,
                    "${:04X} sits at ${:02X}, above the ${:02X} it was called from",
                    pair[1].entry,
                    pair[1].sp,
                    pair[0].sp
                );
            }
        }
        assert!(deepest > 1, "no nested call ran, so nothing was proven");
    }

    /// An NMI arriving part way through an IRQ sequence takes the other vector, so which one was
    /// taken is read back from PC rather than from the flags.
    #[test]
    fn an_interrupt_is_recorded_as_the_vector_it_took() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("test_roms/spritecans.nes")
            .expect("load rom");
        deck.set_call_stack(true);

        let handler = deck.bus().peek_word(Cpu::NMI_VECTOR);
        let mut taken = 0;
        for _ in 0..100_000 {
            deck.clock_instr().expect("clock instruction");
            for frame in deck.call_stack().expect("recording").frames() {
                if frame.kind == FrameKind::Nmi {
                    assert_eq!(frame.entry, handler);
                    taken += 1;
                }
            }
        }
        assert!(taken > 0, "no NMI ran, so nothing was proven");
    }

    #[test]
    fn a_mark_records_its_kind_and_leaves_its_neighbours_alone() {
        let mut map = CodeMap::new(4, 0);
        map.mark(1, ByteKind::CODE);
        assert_eq!(map.kind(1), ByteKind::CODE);
        assert!(map.is_code(1));
        assert_eq!(map.kind(0), ByteKind::empty());
        assert_eq!(map.kind(2), ByteKind::empty());
    }

    #[test]
    fn kinds_accumulate_rather_than_replace_one_another() {
        let mut map = CodeMap::new(1, 0);
        map.mark(0, ByteKind::SUB_ENTRY);
        map.mark(0, ByteKind::CODE);
        assert_eq!(map.kind(0), ByteKind::CODE | ByteKind::SUB_ENTRY);
    }

    /// Callers pass on whatever a page table gave them, so an offset outside the arena the map was
    /// built for has to read as "nothing known" rather than panic.
    #[test]
    fn an_offset_past_the_end_is_ignored() {
        let mut map = CodeMap::new(1, 0);
        map.mark(64, ByteKind::CODE);
        assert_eq!(map.kind(64), ByteKind::empty());
        assert!(!map.is_code(64));
        assert_eq!(map.generation(), 0);
    }

    #[test]
    fn the_generation_moves_only_when_the_map_learns_something() {
        let mut map = CodeMap::new(2, 0);
        map.mark(0, ByteKind::CODE);
        let learned = map.generation();
        assert_ne!(learned, 0, "a first mark is something new");

        map.mark(0, ByteKind::CODE);
        assert_eq!(
            map.generation(),
            learned,
            "re-marking what is already known rebuilds every view for nothing"
        );

        map.mark(0, ByteKind::SUB_ENTRY);
        assert_ne!(
            map.generation(),
            learned,
            "a second kind on a marked byte is still new information"
        );
    }

    /// The write sites in `Bus::clock_instr`: an executed instruction is code, and where a call
    /// leaves PC is the start of a subroutine.
    #[test]
    fn executing_marks_the_instruction_and_the_subroutines_it_calls() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("test_roms/spritecans.nes")
            .expect("load rom");
        deck.attach_code_map(None);

        let mut calls = 0;
        for _ in 0..100_000 {
            let pc = deck.bus().cpu.pc;
            let is_jsr = deck.bus().peek(pc) == Cpu::JSR;
            // Read before the instruction runs: a bank switch inside it would move the byte, and
            // the offset marked is the one that was mapped at the fetch.
            let offset = deck.bus().memory.prg_offset(pc);
            deck.clock_instr().expect("clock instruction");

            let map = deck.code_map().expect("recording");
            if let Some(offset) = offset {
                assert!(map.is_code(offset), "${pc:04X} ran but is not marked code");
            }
            if is_jsr {
                let dest = deck.bus().cpu.pc;
                let offset = deck.bus().memory.prg_offset(dest).expect("mapped");
                assert!(
                    map.kind(offset).contains(ByteKind::SUB_ENTRY),
                    "${dest:04X} was called from ${pc:04X} but is not marked a call target"
                );
                calls += 1;
            }
        }
        assert!(calls > 0, "no subroutine call ran, so nothing was proven");
    }

    /// Closing a debugger stops the recording. What it recorded is still true, so reopening one
    /// resumes rather than starting over.
    #[test]
    fn detaching_keeps_what_was_recorded_for_the_next_attach() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("test_roms/spritecans.nes")
            .expect("load rom");
        deck.attach_code_map(None);
        for _ in 0..1_000 {
            deck.clock_instr().expect("clock instruction");
        }
        let recorded = deck.code_map().expect("recording").generation();
        assert_ne!(recorded, 0);

        let detached = deck.detach_code_map();
        assert!(deck.code_map().is_none(), "still recording after detaching");
        deck.attach_code_map(detached);
        assert_eq!(deck.code_map().expect("recording").generation(), recorded);
    }

    /// Offsets address the arena they were recorded against, so a map from another cart has to be
    /// dropped rather than reused.
    #[test]
    fn a_map_recorded_against_another_cart_is_refused() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("test_roms/spritecans.nes")
            .expect("load rom");
        deck.attach_code_map(None);
        for _ in 0..1_000 {
            deck.clock_instr().expect("clock instruction");
        }
        let detached = deck.detach_code_map();
        assert_ne!(detached.as_ref().expect("recorded").generation(), 0);

        deck.load_rom_path("test_roms/cpu/nestest.nes")
            .expect("load rom");
        deck.attach_code_map(detached);
        assert_eq!(
            deck.code_map().expect("recording").generation(),
            0,
            "another cart's marks were kept"
        );
    }

    /// Marks are memory offsets, so they describe bytes a different cart's arena does not contain,
    /// and that arena is a different size, which would leave the map addressing past its end.
    #[test]
    fn loading_a_cart_starts_the_map_over_at_the_new_size() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("test_roms/spritecans.nes")
            .expect("load rom");
        deck.attach_code_map(None);
        for _ in 0..1_000 {
            deck.clock_instr().expect("clock instruction");
        }
        assert_ne!(deck.code_map().expect("recording").generation(), 0);

        deck.load_rom_path("test_roms/cpu/nestest.nes")
            .expect("load rom");
        let map = deck.code_map().expect("still recording");
        assert_eq!(map.generation(), 0, "marks survived a different cart");
    }

    fn bus_with_cart() -> Bus {
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        cart.mapper = Nrom::load(&mut cart).unwrap();
        bus.load_cart(cart);
        bus.reset(ResetKind::Hard);
        bus
    }

    /// A breakpoint on an address rather than on a bank, the way work RAM records one.
    fn breakpoint(start: u16, end: u16, access: Access) -> Breakpoint {
        Breakpoint {
            start,
            end,
            offset: None,
            access,
            breaks: true,
            condition: None,
        }
    }

    /// The bitmap is only a pre-filter, so a range has to be checked at both ends and one past
    /// each. Getting the inclusive end wrong shortens every range by one address.
    #[test]
    fn a_range_covers_both_ends_and_nothing_past_them() {
        let bus = bus_with_cart();
        let breakpoints = Breakpoints::new([breakpoint(0x0300, 0x0302, Access::WRITE)]);
        for addr in [0x0300, 0x0301, 0x0302] {
            assert!(
                breakpoints.check(&bus, addr, Access::WRITE).stop,
                "${addr:04X}"
            );
        }
        for addr in [0x02FF, 0x0303] {
            assert!(
                !breakpoints.check(&bus, addr, Access::WRITE).stop,
                "${addr:04X}"
            );
        }
    }

    /// A breakpoint stops on the accesses it was ticked for and no others.
    #[test]
    fn an_access_the_breakpoint_does_not_watch_is_ignored() {
        let bus = bus_with_cart();
        let breakpoints = Breakpoints::new([breakpoint(0x0300, 0x0300, Access::WRITE)]);
        assert!(breakpoints.check(&bus, 0x0300, Access::WRITE).stop);
        assert!(!breakpoints.check(&bus, 0x0300, Access::READ).stop);
    }

    /// A condition narrows a breakpoint to the accesses that satisfy it, so a write breakpoint on
    /// a counter can wait for the one write that matters.
    #[test]
    fn a_condition_narrows_what_a_breakpoint_stops_on() {
        let mut bus = bus_with_cart();
        bus.breakpoints_active = true;
        bus.breakpoints = Some(Box::new(Breakpoints::new([Breakpoint {
            condition: Some(Expr::parse("a == 0x42").expect("parses")),
            ..breakpoint(0x0300, 0x0300, Access::WRITE)
        }])));

        bus.cpu.acc = 0x01;
        bus.write(0x0300, 0x99);
        assert_eq!(
            bus.access_hit, None,
            "a condition that does not hold stopped it"
        );

        bus.cpu.acc = 0x42;
        bus.write(0x0300, 0x99);
        assert!(
            bus.access_hit.is_some(),
            "a condition that holds did not stop it"
        );
    }

    /// A breakpoint that records rather than stops keeps the console running, and the accesses
    /// come back once each.
    #[test]
    fn a_recording_breakpoint_collects_hits_without_stopping() {
        let mut bus = bus_with_cart();
        bus.breakpoints_active = true;
        bus.breakpoints = Some(Box::new(Breakpoints::new([Breakpoint {
            breaks: false,
            ..breakpoint(0x0300, 0x0300, Access::WRITE)
        }])));

        bus.write(0x0300, 0x42);
        bus.write(0x0300, 0x43);
        assert_eq!(bus.access_hit, None, "a breakpoint that records stopped it");

        let breakpoints = bus.breakpoints.as_mut().expect("armed");
        let hits = breakpoints.drain_hits();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].value, 0x42);
        assert!(
            breakpoints.drain_hits().is_empty(),
            "draining twice repeats what was already reported"
        );
    }

    /// A breakpoint on a hot address would otherwise grow the log without limit between frames.
    #[test]
    fn a_recording_breakpoint_stops_collecting_at_its_cap() {
        let mut breakpoints = Breakpoints::new([Breakpoint {
            breaks: false,
            ..breakpoint(0x0300, 0x0300, Access::WRITE)
        }]);
        for _ in 0..Breakpoints::MAX_HITS * 2 {
            breakpoints.record(AccessHit {
                pc: 0xC000,
                addr: 0x0300,
                access: Access::WRITE,
                value: 0,
            });
        }
        assert_eq!(breakpoints.drain_hits().len(), Breakpoints::MAX_HITS);
    }

    /// Past the cap the extras are dropped rather than scanned, so the work a hit pays for stays
    /// bounded.
    #[test]
    fn breakpoints_past_the_cap_are_dropped() {
        let breakpoints = Breakpoints::new(
            (0..Breakpoints::MAX as u16 + 8)
                .map(|i| breakpoint(0x0300 + i, 0x0300 + i, Access::WRITE)),
        );
        assert_eq!(breakpoints.list.len(), Breakpoints::MAX);
    }

    /// The debugger resolves every row it draws through `peek`, so a read breakpoint that fired
    /// there would trip on the disassembly reading the address it was set on.
    #[test]
    fn peeking_does_not_trip_a_read_breakpoint() {
        let mut bus = bus_with_cart();
        bus.breakpoints_active = true;
        bus.breakpoints = Some(Box::new(Breakpoints::new([breakpoint(
            0x0300,
            0x0300,
            Access::READ | Access::WRITE,
        )])));

        let _ = bus.peek(0x0300);
        assert_eq!(bus.access_hit, None);

        let _ = bus.read(0x0300);
        assert!(bus.access_hit.is_some(), "a real read still stops");
    }

    /// A read-modify-write puts the old value back before the new one, and only the first hit is
    /// kept, so reporting the re-write names a value the program never chose.
    #[test]
    fn a_write_breakpoint_reports_the_value_the_program_wrote() {
        let mut bus = bus_with_cart();
        bus.cpu_bus_write(0x0010, 0x05);
        // `INC $10`, which reads $10, writes $05 back, then writes $06.
        bus.cpu_bus_write(0x0000, 0xE6);
        bus.cpu_bus_write(0x0001, 0x10);
        bus.cpu.pc = 0x0000;
        bus.breakpoints_active = true;
        bus.breakpoints = Some(Box::new(Breakpoints::new([breakpoint(
            0x0010,
            0x0010,
            Access::WRITE,
        )])));

        bus.clock_instr();
        let hit = bus.access_hit.expect("the write was caught");
        assert_eq!(
            hit.value, 0x06,
            "the re-write of the old value was reported instead"
        );
        assert_eq!(
            hit.pc, 0x0000,
            "the hit names where PC reached, not the instruction that wrote"
        );
    }

    /// A breakpoint that records rather than stops is the only one served on execution, and
    /// nothing else on the bus can tell an instruction fetch from an operand read.
    #[test]
    fn a_recording_breakpoint_logs_each_execution() {
        let mut bus = bus_with_cart();
        // `LDA #$42`, run twice from the same address.
        bus.cpu_bus_write(0x0000, 0xA9);
        bus.cpu_bus_write(0x0001, 0x42);
        bus.breakpoints_active = true;
        bus.breakpoints = Some(Box::new(Breakpoints::new([Breakpoint {
            breaks: false,
            ..breakpoint(0x0000, 0x0000, Access::EXEC)
        }])));

        for _ in 0..2 {
            bus.cpu.pc = 0x0000;
            bus.clock_instr();
        }

        let hits = bus.breakpoints.as_mut().expect("armed").drain_hits();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].access, Access::EXEC);
        assert_eq!(hits[0].addr, 0x0000);
        assert_eq!(hits[0].value, 0xA9, "the opcode is what executed");
        assert_eq!(bus.access_hit, None, "a breakpoint that records stopped it");
    }

    /// `Access::EXEC` covers fetches, so a read breakpoint over a bank holding both code and data
    /// would otherwise stop on every instruction in it.
    #[test]
    fn a_read_breakpoint_does_not_fire_on_an_instruction_fetch() {
        let mut bus = bus_with_cart();
        // `LDA #$42`, whose two bytes are both fetches and whose operand is immediate.
        bus.cpu_bus_write(0x0000, 0xA9);
        bus.cpu_bus_write(0x0001, 0x42);
        bus.cpu.pc = 0x0000;
        bus.breakpoints_active = true;
        bus.breakpoints = Some(Box::new(Breakpoints::new([breakpoint(
            0x0000,
            0x0001,
            Access::READ,
        )])));

        bus.clock_instr();
        assert_eq!(bus.access_hit, None);
    }

    /// With nothing armed the read and write paths take one `bool` test and no further work,
    /// which is all the hot path is allowed to spend.
    #[test]
    fn nothing_armed_leaves_the_bus_unwatched() {
        let mut bus = bus_with_cart();
        let _ = bus.read(0x0300);
        bus.write(0x0300, 0x42);
        assert!(!bus.breakpoints_active);
        assert!(bus.breakpoints.is_none());
        assert_eq!(bus.access_hit, None);
    }

    /// Two 16 KiB PRG banks mapped flat, so a test can swap the second into `$8000`.
    fn bus_with_banked_cart() -> Bus {
        let mut bus = Bus::default();
        let mut cart = Cart::empty_sized(2 * BANK, 0x2000);
        cart.mapper = Nrom::load(&mut cart).unwrap();
        bus.load_cart(cart);
        bus.reset(ResetKind::Hard);
        bus
    }

    /// A breakpoint names the bytes it was set over. Another bank at the same address holds
    /// unrelated code, and stopping there stops somewhere nothing was set.
    #[test]
    fn a_breakpoint_does_not_fire_on_another_bank_at_its_address() {
        let mut bus = bus_with_banked_cart();
        let offset = bus.memory.prg_offset(0x8000).expect("mapped") as u32;
        let breakpoint = Breakpoint {
            offset: Some(offset),
            ..breakpoint(0x8000, 0x8000, Access::EXEC)
        };
        assert!(breakpoint.matches(&bus.memory, 0x8000, Access::EXEC));

        bus.memory.map_prg(0x8000, BANK, 1, Src::PrgRom);
        assert!(!breakpoint.matches(&bus.memory, 0x8000, Access::EXEC));
        assert!(
            breakpoint.covers(0x8000, Access::EXEC),
            "the address is still covered, so the bank is what refused it"
        );
    }

    /// A range answers to the bank at its start, so switching that bank out takes the whole range
    /// with it however far past the switched window the range reaches.
    #[test]
    fn a_range_is_pinned_by_the_bank_at_its_start() {
        let mut bus = bus_with_banked_cart();
        let offset = bus.memory.prg_offset(0xBFFF).expect("mapped") as u32;
        let breakpoint = Breakpoint {
            offset: Some(offset),
            ..breakpoint(0xBFFF, 0xC000, Access::EXEC)
        };
        assert!(breakpoint.matches(&bus.memory, 0xC000, Access::EXEC));

        bus.memory.map_prg(0x8000, BANK, 1, Src::PrgRom);
        assert!(!breakpoint.matches(&bus.memory, 0xBFFF, Access::EXEC));
        assert!(!breakpoint.matches(&bus.memory, 0xC000, Access::EXEC));
    }

    /// Work RAM has no arena offset to pin to, so a breakpoint there is keyed by address alone.
    #[test]
    fn a_breakpoint_outside_the_cart_is_keyed_by_address() {
        let bus = bus_with_banked_cart();
        assert_eq!(bus.memory.prg_offset(0x0300), None);
        assert!(breakpoint(0x0300, 0x0300, Access::WRITE).matches(
            &bus.memory,
            0x0300,
            Access::WRITE
        ));
    }
}
