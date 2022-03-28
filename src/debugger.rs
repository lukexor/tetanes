use crate::{cpu::Cpu, memory::MemAccess};
use std::ops::RangeInclusive;

// TODO: Use Address
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Address {
    Addr(u16),
    AddrRange(RangeInclusive<u16>),
}

// Conditions:
// - A/X/Y/P/SP
// - PC
// - Opcode
// - Scanline
// - Cycle
// - Memory value
// - Branched
// - IRQ/NMI
// - Spr0 Hit/Spr Overflow
// - VBlank
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct Condition {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Breakpoint {
    pub(crate) addr: Address,
    pub(crate) access: Vec<MemAccess>,
    pub(crate) conditions: Vec<Condition>,
    pub(crate) enabled: bool,
}

impl Breakpoint {
    pub(crate) const fn matches(&self, _cpu: &Cpu) -> bool {
        // TODO: Breakpoint matching
        false
    }
}
