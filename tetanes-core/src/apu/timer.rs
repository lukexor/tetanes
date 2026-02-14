//! Timer abstraction for the [`Apu`](crate::apu::Apu).

use crate::common::{Reset, ResetKind};
use serde::{Deserialize, Serialize};

/// Trait for types that have timers.
pub trait TimerCycle {
    fn cycle(&self) -> u32;
}

/// A timer that generates a clock signal based on a divider and a period. The timer is clocked
/// every (period + 1) * divider cycles.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Timer {
    pub cycle: u32,
    pub counter: u16,
    pub period: u16,
}

impl Timer {
    pub const fn new(period: u16) -> Self {
        Self {
            cycle: 0,
            counter: 0,
            period,
        }
    }

    pub const fn preload(period: u16) -> Self {
        let mut timer = Self::new(period);
        timer.counter = timer.period;
        timer
    }

    pub const fn reload(&mut self) {
        self.counter = self.period;
    }

    pub const fn tick(&mut self) -> bool {
        self.cycle += 1;
        if self.counter == 0 {
            self.counter = self.period;
            return true;
        }
        self.counter -= 1;
        false
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
        let mut expected = [false; 23];
        expected[0] = true;
        expected[11] = true;
        expected[22] = true;
        assert_eq!(expected, [(); 23].map(|_| timer.tick()));
        assert_eq!(23, timer.cycle);

        // Period (10 + 1) == 11
        let mut timer = Timer::preload(10);
        let mut expected = [false; 22];
        expected[10] = true;
        expected[21] = true;
        assert_eq!(expected, [(); 22].map(|_| timer.tick()));
        assert_eq!(22, timer.cycle);

        // Period (10 * 2) + 1 == 22 + initial clock
        let mut timer = Timer::new((10 * 2) + 1);
        let mut expected = [false; 45];
        expected[0] = true;
        expected[22] = true;
        expected[44] = true;
        assert_eq!(expected, [(); 45].map(|_| timer.tick()));
        assert_eq!(45, timer.cycle);

        // Period (10 * 2) + 1 == 22
        let mut timer = Timer::preload((10 * 2) + 1);
        let mut expected = [false; 44];
        expected[21] = true;
        expected[43] = true;
        assert_eq!(expected, [(); 44].map(|_| timer.tick()));
        assert_eq!(44, timer.cycle);
    }
}
