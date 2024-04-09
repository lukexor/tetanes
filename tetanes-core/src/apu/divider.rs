use crate::common::{Clock, ClockTo, Reset, ResetKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Divider {
    pub cycle: usize,
    pub divisor: usize,
}

impl Default for Divider {
    fn default() -> Self {
        Self {
            cycle: 0,
            divisor: 1,
        }
    }
}

impl Divider {
    pub const fn new(divisor: usize) -> Self {
        Self { cycle: 0, divisor }
    }
}

impl Clock for Divider {
    fn clock(&mut self) -> usize {
        self.clock_to(self.cycle + 1)
    }
}

impl ClockTo for Divider {
    fn clock_to(&mut self, cycle: usize) -> usize {
        self.cycle = cycle;
        let mut cycles = 0;
        while self.cycle >= self.divisor {
            self.cycle -= self.divisor;
            cycles += 1;
        }
        cycles
    }
}

impl Reset for Divider {
    fn reset(&mut self, _kind: ResetKind) {
        self.cycle = 0;
    }
}
