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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider() {
        let mut divider = Divider::new(1);
        let expected = [1; 5];
        assert_eq!(expected, [(); 5].map(|_| divider.clock()));

        let mut divider = Divider::new(2);
        let mut expected = [0; 5];
        expected[1] = 1;
        expected[3] = 1;
        assert_eq!(expected, [(); 5].map(|_| divider.clock()));

        let mut divider = Divider::new(3);
        let mut expected = [0; 6];
        expected[2] = 1;
        expected[5] = 1;
        assert_eq!(expected, [(); 6].map(|_| divider.clock()));

        let mut divider = Divider::new(4);
        let mut expected = [0; 8];
        expected[3] = 1;
        expected[7] = 1;
        assert_eq!(expected, [(); 8].map(|_| divider.clock()));

        let mut divider = Divider::new(5);
        let mut expected = [0; 10];
        expected[4] = 1;
        expected[9] = 1;
        assert_eq!(expected, [(); 10].map(|_| divider.clock()));
    }
}
