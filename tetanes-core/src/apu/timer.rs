//! Timer abstraction for the [`Apu`](crate::apu::Apu).

use crate::common::{Clock, ClockTo, Reset, ResetKind};
use serde::{Deserialize, Serialize};

/// Trait for types that have timers.
pub trait TimerCycle {
    fn cycle(&self) -> u64;
}

/// A timer that generates a clock signal based on a divider and a period. The timer is clocked
/// every (period + 1) * divider cycles.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Timer {
    pub cycle: u64,
    pub counter: u64,
    pub period: u64,
}

impl Timer {
    pub const fn new(period: u64) -> Self {
        Self {
            cycle: 0,
            counter: 0,
            period,
        }
    }

    pub const fn preload(period: u64) -> Self {
        let mut timer = Self::new(period);
        timer.counter = timer.period;
        timer
    }

    pub const fn reload(&mut self) {
        self.counter = self.period;
    }
}

impl Clock for Timer {
    fn clock(&mut self) -> u64 {
        self.clock_to(self.cycle + 1)
    }
}

impl ClockTo for Timer {
    fn clock_to(&mut self, cycle: u64) -> u64 {
        let cycles = cycle - self.cycle;
        if cycles > self.counter {
            self.cycle += self.counter + 1;
            self.counter = self.period;
            return 1;
        }
        self.counter -= cycles;
        self.cycle = cycle;
        0
    }
}

impl Reset for Timer {
    fn reset(&mut self, _kind: ResetKind) {
        self.counter = 0;
        self.period = 0;
        self.cycle = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer() {
        // Period (10 + 1) == 11 + initial clock
        let mut timer = Timer::new(10);
        let mut expected = [0; 23];
        expected[0] = 1;
        expected[11] = 1;
        expected[22] = 1;
        assert_eq!(expected, [(); 23].map(|_| timer.clock()));

        // Period (10 + 1) == 11
        let mut timer = Timer::preload(10);
        let mut expected = [0; 22];
        expected[10] = 1;
        expected[21] = 1;
        assert_eq!(expected, [(); 22].map(|_| timer.clock()));

        // Period (10 * 2) + 1 == 22 + initial clock
        let mut timer = Timer::new((10 * 2) + 1);
        let mut expected = [0; 45];
        expected[0] = 1;
        expected[22] = 1;
        expected[44] = 1;
        assert_eq!(expected, [(); 45].map(|_| timer.clock()));

        // Period (10 * 2) + 1 == 22
        let mut timer = Timer::preload((10 * 2) + 1);
        let mut expected = [0; 44];
        expected[21] = 1;
        expected[43] = 1;
        assert_eq!(expected, [(); 44].map(|_| timer.clock()));
    }
}
