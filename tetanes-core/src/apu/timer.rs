//! Timer abstraction for the [`Apu`](crate::apu::Apu).

// The APU's internal wiring: channel state, dividers and filter taps, whose meaning is the
// hardware's rather than this crate's. Public for embedders and debuggers, not a stable surface -
// see the module docs on `apu`.
#![allow(missing_docs)]

use crate::common::ResetKind;
use serde::{Deserialize, Serialize};

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

    /// Advance toward `target`, stopping at each expiry.
    ///
    /// Returns `true` having advanced to the cycle the divider expired on, so a caller drives it
    /// as `while timer.run_to(target) { ..what an expiry does.. }`; the `false` that ends the loop
    /// leaves the timer at `target`.
    ///
    /// This is [`Timer::tick`] with the cycles in between collapsed into one subtraction, and it is
    /// why the channels are not clocked once per CPU cycle: a pulse at period 200 expires every
    /// ~400 cycles, so a 10,000-cycle block is ~25 iterations rather than 10,000. [`Timer::tick`]
    /// remains for the boards that clock a channel from their own per-cycle hook (MMC5, VRC6).
    pub const fn run_to(&mut self, target: u32) -> bool {
        // A channel can sit *ahead* of the cycle being asked for: `Dmc::reset` parks its timer one
        // cycle forward on purpose (see the FIXME there) and `Triangle::reset` leaves its timer
        // alone entirely, both while the APU's own counters go back to zero. The per-cycle
        // `while cycle() < target` loop this replaces did nothing in that case, so neither does
        // this - without the guard the subtraction wraps and fires a spurious expiry.
        if target <= self.cycle {
            return false;
        }
        let remaining = target - self.cycle;
        if remaining > self.counter as u32 {
            // `counter + 1` cycles to expiry, matching `tick`: the counter is decremented on every
            // cycle but the one it reads zero on, which is the one that fires.
            self.cycle += self.counter as u32 + 1;
            self.counter = self.period;
            return true;
        }
        self.counter -= remaining as u16;
        self.cycle = target;
        false
    }
    pub const fn reset(&mut self, _kind: ResetKind) {
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

    /// `run_to` exists only to collapse the cycles between expiries, so what it must never do is
    /// land on a different set of them. Both walk the same timer over the same span here, at every
    /// period a channel actually uses and from every starting phase.
    #[test]
    fn run_to_expires_on_the_same_cycles_as_tick() {
        for period in [0, 1, 2, 7, 21, 200, 2033, 4067] {
            for preload in [false, true] {
                let mut ticked = if preload {
                    Timer::preload(period)
                } else {
                    Timer::new(period)
                };
                let mut expiries = Vec::new();
                for _ in 0..10_000 {
                    if ticked.tick() {
                        expiries.push(ticked.cycle);
                    }
                }

                // Every span length, so the jump lands mid-period as well as exactly on one.
                for span in [1, 2, 3, 17, 64, 10_000] {
                    let mut run = if preload {
                        Timer::preload(period)
                    } else {
                        Timer::new(period)
                    };
                    let mut actual = Vec::new();
                    let mut target = 0;
                    while target < 10_000 {
                        target = (target + span).min(10_000);
                        while run.run_to(target) {
                            actual.push(run.cycle);
                        }
                    }
                    assert_eq!(
                        actual, expiries,
                        "period {period}, preload {preload}, span {span}"
                    );
                    assert_eq!(run.cycle, ticked.cycle, "timer must end at the target");
                    assert_eq!(
                        run.counter, ticked.counter,
                        "and mid-period at the same point"
                    );
                }
            }
        }
    }

    /// `Dmc::reset` parks its timer a cycle ahead of the APU's counters on purpose, so a target
    /// behind the timer is a real state, not a bug. It has to be a no-op, the way the per-cycle
    /// loop that preceded `run_to` was.
    #[test]
    fn run_to_a_target_already_passed_does_nothing() {
        let mut timer = Timer::preload(20);
        for _ in 0..30 {
            timer.tick();
        }
        let (cycle, counter) = (timer.cycle, timer.counter);
        assert!(cycle > 0);

        for target in 0..cycle {
            assert!(!timer.run_to(target), "must not expire at {target}");
            assert_eq!(timer.cycle, cycle, "and must not move");
            assert_eq!(timer.counter, counter);
        }
    }
}
