//! Debugger hooks.
//!
//! A debugger is a callback plus the condition that fires it. The callback is handed the whole
//! [`Bus`](crate::bus::Bus) - every register, both address spaces, the board - rather than one component's state,
//! because what a debugger needs differs per debugger: a CPU debugger wants registers and the
//! disassembly around PC, an APU viewer wants the channels, a hex viewer wants an arbitrary range,
//! and the PPU viewer wants CHR resolved through the board. Handing over the console lets each one
//! take exactly what it needs at the point where the state is coherent, and is also what a future
//! breakpoint *condition* needs in order to decide.

use crate::bus::Bus;
use std::sync::Arc;

/// Runs a callback once the PPU reaches a given dot, handing it the console.
///
/// The dot is the only trigger for now; the shape to extend is this struct - a `Trigger` enum
/// (`Dot`, `Pc`, `Read`) - not the callback, which already sees everything.
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

impl std::fmt::Debug for Debugger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Debugger")
            .field("cycle", &self.cycle)
            .field("scanline", &self.scanline)
            .finish_non_exhaustive()
    }
}
