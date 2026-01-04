use crate::ppu::Ppu;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum Debugger {
    Ppu(PpuDebugger),
}

impl From<PpuDebugger> for Debugger {
    fn from(debugger: PpuDebugger) -> Self {
        Self::Ppu(debugger)
    }
}

#[derive(Clone)]
#[must_use]
pub struct PpuDebugger {
    pub cycle: u32,
    pub scanline: u32,
    pub callback: Arc<dyn Fn(Ppu) + Send + Sync + 'static>,
}

impl Default for PpuDebugger {
    fn default() -> Self {
        Self {
            cycle: u32::MAX,
            scanline: u32::MAX,
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
