//! `VrcIrq`
//!
//! <https://www.nesdev.org/wiki/VRC_IRQ>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::common::ResetKind;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct VrcIrq {
    pub reload: u8,
    pub counter: u8,
    pub prescalar_counter: i16,
    pub irq_pending: bool,
    pub enabled: bool,
    pub enabled_after_ack: bool,
    pub cycle_mode: bool,
}

impl VrcIrq {
    pub const fn write_reload(&mut self, val: u8) {
        self.reload = val;
    }

    /// VRC4 splits the reload value across two registers, low nibble first.
    pub const fn write_reload_lo(&mut self, val: u8) {
        self.reload = (self.reload & 0xF0) | (val & 0x0F);
    }

    pub const fn write_reload_hi(&mut self, val: u8) {
        self.reload = (self.reload & 0x0F) | ((val & 0x0F) << 4);
    }

    pub const fn write_control(&mut self, val: u8) {
        self.enabled_after_ack = val & 0x01 == 0x01;
        self.enabled = val & 0x02 == 0x02;
        self.cycle_mode = val & 0x04 == 0x04;

        if self.enabled {
            self.counter = self.reload;
            self.prescalar_counter = 341;
        }

        self.irq_pending = false;
    }

    pub const fn acknowledge(&mut self) {
        self.enabled = self.enabled_after_ack;
        self.irq_pending = false;
    }
    #[inline]
    pub const fn clock(&mut self) {
        if !self.enabled {
            return;
        }
        if self.cycle_mode {
            // The prescaler is not part of cycle mode and must be left alone. Running it here used
            // to net +338 per clock, which overflows `i16` - and panics a debug build - after
            // ~97 CPU cycles.
            self.tick();
        } else {
            self.prescalar_counter -= 3;
            if self.prescalar_counter <= 0 {
                // Carried, not reset, so three ticks cost 341 clocks like three real scanlines.
                self.prescalar_counter += 341;
                self.tick();
            }
        }
    }

    /// Steps the up-counter, asserting the IRQ on the wrap out of `$FF`.
    const fn tick(&mut self) {
        if self.counter == 0xFF {
            self.counter = self.reload;
            self.irq_pending = true;
        } else {
            self.counter += 1;
        }
    }
    pub const fn reset(&mut self, _kind: ResetKind) {
        self.reload = 0;
        self.counter = 0;
        self.prescalar_counter = 0;
        self.enabled = false;
        self.enabled_after_ack = false;
        self.cycle_mode = false;
        // Without this a reset leaves the line asserted forever: every other field is cleared, so
        // nothing can ever reach the acknowledge that would clear it.
        self.irq_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enable the counter in scanline mode with the given reload value, staying enabled across an
    /// acknowledge so a test can measure successive ticks.
    fn scanline_mode(reload: u8) -> VrcIrq {
        let mut irq = VrcIrq::default();
        irq.write_reload(reload);
        irq.write_control(0x03);
        irq
    }

    /// Clocks until the IRQ fires, returning how many clocks that took, or `None` past `limit`.
    fn clocks_to_irq(irq: &mut VrcIrq, limit: u32) -> Option<u32> {
        (1..=limit).find(|_| {
            irq.clock();
            irq.irq_pending
        })
    }

    /// The counter is an up-counter that fires on the wrap out of `$FF`, so a reload of `n` gives
    /// `256 - n` ticks - not `n`.
    #[test]
    fn cycle_mode_fires_after_256_minus_reload_clocks() {
        for (reload, expected) in [(0xFF, 1), (0xF0, 16), (0x00, 256)] {
            let mut irq = VrcIrq::default();
            irq.write_reload(reload);
            irq.write_control(0x06); // enable + cycle mode
            assert_eq!(
                clocks_to_irq(&mut irq, 300),
                Some(expected),
                "reload ${reload:02X} in cycle mode"
            );
        }
    }

    /// In scanline mode the prescaler divides the CPU clock by 341/3, i.e. a tick every 114 CPU
    /// cycles, and carries its remainder rather than resetting - so the ticks are not all 114 apart.
    #[test]
    fn scanline_mode_ticks_every_114ish_clocks_carrying_the_remainder() {
        let mut irq = scanline_mode(0xFF);
        assert_eq!(clocks_to_irq(&mut irq, 500), Some(114), "first tick");

        // 341 - 3*114 = -1, so the second period is one clock shorter, and the third shorter again.
        irq.acknowledge();
        assert_eq!(clocks_to_irq(&mut irq, 500), Some(114), "second tick");
        irq.acknowledge();
        assert_eq!(clocks_to_irq(&mut irq, 500), Some(113), "third tick");
    }

    /// Three scanline ticks must cost the same as three real scanlines - 341 PPU dots each, or 341
    /// CPU cycles for three of them - which is exactly what carrying the remainder buys.
    #[test]
    fn three_scanline_ticks_cost_341_clocks() {
        let mut irq = scanline_mode(0xFF);
        let mut total = 0;
        for _ in 0..3 {
            total += clocks_to_irq(&mut irq, 500).expect("fires");
            irq.acknowledge();
        }
        assert_eq!(total, 341);
    }

    /// Bit 1 enables the counter now; bit 0 is what it goes back to on acknowledge. That split is
    /// the whole point of the register - it lets a handler re-arm without another write.
    #[test]
    fn acknowledge_restores_enable_from_the_after_ack_bit() {
        let mut irq = VrcIrq::default();
        irq.write_reload(0xFF);

        irq.write_control(0x03); // enable, and stay enabled after ack
        irq.clock_until_pending();
        irq.acknowledge();
        assert!(!irq.irq_pending, "acknowledge clears the line");
        assert!(irq.enabled, "bit 0 set means the counter keeps running");

        irq.write_control(0x02); // enable, but stop after ack
        irq.clock_until_pending();
        irq.acknowledge();
        assert!(
            !irq.enabled,
            "bit 0 clear means acknowledge disables the counter"
        );
        // And a disabled counter really is inert.
        assert_eq!(clocks_to_irq(&mut irq, 1000), None);
    }

    /// Writing the control register reloads the counter and restarts the prescaler, so a write
    /// mid-count is a full restart rather than a resume.
    #[test]
    fn enabling_reloads_the_counter_and_restarts_the_prescaler() {
        let mut irq = scanline_mode(0xFF);
        for _ in 0..100 {
            irq.clock();
        }
        irq.write_control(0x02);
        assert_eq!(
            clocks_to_irq(&mut irq, 500),
            Some(114),
            "a full period, not the 14 clocks left over"
        );
    }

    /// A disabled counter does not run at all, and enabling never leaves a stale line asserted.
    #[test]
    fn a_disabled_counter_neither_counts_nor_leaves_the_line_asserted() {
        let mut irq = VrcIrq::default();
        irq.write_reload(0xFF);
        assert_eq!(clocks_to_irq(&mut irq, 1000), None, "never enabled");

        irq.write_control(0x06);
        irq.clock_until_pending();
        irq.write_control(0x00);
        assert!(!irq.irq_pending, "writing control clears the line");
    }

    /// Reset has to clear `irq_pending` along with every other field. Leaving the line asserted
    /// leaves no way to ever clear it: the counter is disabled, so nothing reaches an acknowledge.
    #[test]
    fn reset_clears_a_pending_irq() {
        let mut irq = VrcIrq::default();
        irq.write_reload(0xFF);
        irq.write_control(0x06);
        irq.clock_until_pending();

        irq.reset(ResetKind::Soft);
        assert!(!irq.irq_pending, "a reset must drop the IRQ line");
        assert!(!irq.enabled);
    }

    impl VrcIrq {
        /// Clocks until the IRQ asserts, panicking rather than spinning if it never does.
        fn clock_until_pending(&mut self) {
            assert!(
                clocks_to_irq(self, 1000).is_some(),
                "expected the counter to fire within 1000 clocks"
            );
        }
    }
}
