//! Debugger hooks.
//!
//! A debugger is a callback plus the condition that fires it. The callback is handed the whole
//! [`Bus`](crate::bus::Bus) - so it can take whatever state snapshot it needs at that point during
//! emulation.

use crate::bus::Bus;
use std::sync::Arc;

/// A ring buffer of the program counters most recently executed.
///
/// Executed instructions have to be recorded as it runs: 6502 instructions are one to three bytes
/// with no alignment, so the stream before an address cannot be recovered by decoding backwards
/// without a known address to disassemble from. This is what lets a debugger show the instructions
/// leading up to where it stopped execution.
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
    use super::PcHistory;

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
}
