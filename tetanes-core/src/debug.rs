use crate::ppu::Ppu;
use std::sync::Arc;

/// A debugger attached to one component.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum Debugger {
    /// A callback run at a chosen PPU dot.
    Ppu(PpuDebugger),
}

impl From<PpuDebugger> for Debugger {
    fn from(debugger: PpuDebugger) -> Self {
        Self::Ppu(debugger)
    }
}

/// Runs a callback once the PPU reaches a given dot, handing it a copy of the PPU.
#[derive(Clone)]
#[must_use]
pub struct PpuDebugger {
    /// The cycle within `scanline` to break on.
    pub cycle: u16,
    /// The scanline to break on.
    pub scanline: u16,
    /// What to run when that dot is reached.
    pub callback: Arc<dyn Fn(Ppu) + Send + Sync + 'static>,
}

impl Default for PpuDebugger {
    fn default() -> Self {
        Self {
            cycle: u16::MAX,
            scanline: u16::MAX,
            callback: Arc::new(|_| {}),
        }
    }
}

impl PartialEq for PpuDebugger {
    fn eq(&self, other: &Self) -> bool {
        self.cycle == other.cycle && self.scanline == other.scanline
    }
}

impl std::fmt::Debug for PpuDebugger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PpuDebugger")
            .field("cycle", &self.cycle)
            .field("scanline", &self.scanline)
            .finish_non_exhaustive()
    }
}
