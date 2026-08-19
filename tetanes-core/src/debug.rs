//! Debugger hooks.
//!
//! A debugger is a callback plus the condition that fires it. The callback is handed the whole
//! [`Bus`](crate::bus::Bus) - so it can take whatever state snapshot it needs at that point during
//! emulation.

use crate::{bus::Bus, memory::Memory};
use bitflags::bitflags;
use std::sync::Arc;

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
    use super::{ByteKind, CodeMap, PcHistory};
    use crate::{control_deck::ControlDeck, cpu::Cpu};

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
}
