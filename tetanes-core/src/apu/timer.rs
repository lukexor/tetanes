use crate::{
    apu::divider::Divider,
    common::{Clock, Reset, ResetKind},
};
use serde::{Deserialize, Serialize};

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
        self.cycle += 1;
        if self.divider.clock() > 0 {
            if self.counter > 0 {
                self.counter -= 1;
            } else {
                self.counter = self.period;
                return 1;
            }
        }
        0
    }
}

impl Reset for Timer {
    fn reset(&mut self, kind: ResetKind) {
        self.divider.reset(kind);
        self.counter = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer() {
        let mut timer = Timer::new(10, 1);
        assert_eq!(timer.clock(), 1, "{timer:?}");
        for _ in 0..10 {
            assert_eq!(timer.clock(), 0, "{timer:?}");
        }
        assert_eq!(timer.clock(), 1, "{timer:?}");
        assert_eq!(timer.cycle, 12, "{timer:?}");

        let mut timer = Timer::new(10, 2);
        assert_eq!(timer.clock(), 0, "{timer:?}");
        assert_eq!(timer.clock(), 1, "{timer:?}");
        for _ in 0..21 {
            assert_eq!(timer.clock(), 0, "{timer:?}");
        }
        assert_eq!(timer.clock(), 1, "{timer:?}");
        assert_eq!(timer.cycle, 24, "{timer:?}");
    }

    #[test]
    fn timer_preload() {
        let mut timer = Timer::preload(10, 1);
        for _ in 0..10 {
            assert_eq!(timer.clock(), 0, "{timer:?}");
        }
        assert_eq!(timer.clock(), 1, "{timer:?}");
        assert_eq!(timer.cycle, 11, "{timer:?}");

        let mut timer = Timer::preload(10, 2);
        for _ in 0..21 {
            assert_eq!(timer.clock(), 0, "{timer:?}");
        }
        assert_eq!(timer.clock(), 1, "{timer:?}");
        assert_eq!(timer.cycle, 22, "{timer:?}");
    }
}
