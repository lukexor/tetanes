use crate::{
    apu::divider::Divider,
    common::{Clock, ClockTo, Reset, ResetKind},
};
use serde::{Deserialize, Serialize};

/// Trait for types that have timers.
pub trait TimerCycle {
    fn cycle(&self) -> usize;
}

/// A timer that generates a clock signal based on a divider and a period. The timer is clocked
/// every (period + 1) * divider cycles.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Timer {
    pub cycle: usize,
    pub counter: usize,
    pub period: usize,
    pub divider: Divider,
}

impl Timer {
    pub const fn new(period: usize, divisor: usize) -> Self {
        Self {
            cycle: 0,
            counter: 0,
            period,
            divider: Divider::new(divisor),
        }
    }

    pub const fn preload(period: usize, divisor: usize) -> Self {
        let mut timer = Self::new(period, divisor);
        timer.counter = timer.period;
        timer
    }

    pub fn reload(&mut self) {
        self.counter = self.period;
    }
}

impl Clock for Timer {
    fn clock(&mut self) -> usize {
        self.clock_to(self.cycle + 1)
    }
}

impl ClockTo for Timer {
    fn clock_to(&mut self, cycle: usize) -> usize {
        if self.divider.clock() > 0 {
            let cycles = cycle - self.cycle;
            if cycles > self.counter {
                self.cycle += self.counter + 1;
                self.counter = self.period;
                return 1;
            }
            self.counter -= cycles;
        }
        self.cycle = cycle;
        0
    }
}

impl Reset for Timer {
    fn reset(&mut self, kind: ResetKind) {
        self.counter = 0;
        self.period = 0;
        self.cycle = 0;
        self.divider.reset(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer() {
        // Period (10 + 1) == 11 + initial clock
        let mut timer = Timer::new(10, 1);
        let mut expected = [0; 23];
        expected[0] = 1;
        expected[11] = 1;
        expected[22] = 1;
        assert_eq!(expected, [(); 23].map(|_| timer.clock()));

        // Period (10 + 1) == 11
        let mut timer = Timer::preload(10, 1);
        let mut expected = [0; 22];
        expected[10] = 1;
        expected[21] = 1;
        assert_eq!(expected, [(); 22].map(|_| timer.clock()));

        // Period (10 + 1) * 2 == 22 + initial delayed clock
        let mut timer = Timer::new(10, 2);
        let mut expected = [0; 46];
        expected[1] = 1;
        expected[23] = 1;
        expected[45] = 1;
        assert_eq!(expected, [(); 46].map(|_| timer.clock()));

        // Period (10 + 1) * 2 == 22
        let mut timer = Timer::preload(10, 2);
        let mut expected = [0; 44];
        expected[21] = 1;
        expected[43] = 1;
        assert_eq!(expected, [(); 44].map(|_| timer.clock()));

        // Period (11 + 1) * 3 == 36 + initial delayed clock
        let mut timer = Timer::new(11, 3);
        let mut expected = [0; 75];
        expected[2] = 1;
        expected[38] = 1;
        expected[74] = 1;
        assert_eq!(expected, [(); 75].map(|_| timer.clock()));

        // Period (11 + 1) * 3 == 36
        let mut timer = Timer::preload(11, 3);
        let mut expected = [0; 72];
        expected[35] = 1;
        expected[71] = 1;
        assert_eq!(expected, [(); 72].map(|_| timer.clock()));
    }
}
