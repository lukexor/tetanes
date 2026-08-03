//! Debugger hooks.
//!
//! A debugger is a callback plus the condition that fires it. The callback is handed the whole
//! [`Bus`](crate::bus::Bus) - every register, both address spaces, the board - so it can take
//! whatever it needs at the point where the state is coherent: registers and the disassembly
//! around PC, the APU's channels, a range of memory, or CHR resolved through the board.

use crate::bus::Bus;
use std::sync::Arc;

/// Runs a callback once the PPU reaches a given dot, handing it the console.
#[derive(Clone)]
#[must_use]
pub struct Debugger {
    /// The cycle within `scanline` to break on.
    pub cycle: u16,
    /// The scanline to break on.
    pub scanline: u16,
    /// What to run when that dot is reached.
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
    /// Attach (or clear, via [`Debugger::default`]) a debugger callback.
    //
    // Recomputes the cached `debugger_active` flag so the per-dot path tests one bool instead of
    // touching the cold `debugger` field when nothing is attached.
    #[inline]
    pub fn set_debugger(&mut self, debugger: Debugger) {
        self.debugger_active = debugger != Debugger::default();
        self.debugger = debugger;
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
