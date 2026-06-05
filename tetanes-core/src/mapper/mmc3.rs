//! `MMC3` shared register file, IRQ counter, and bank-select primitives.
//!
//! The `MMC3` mapper family (`TxROM` and its many derivatives) shares a common
//! register file, scanline IRQ counter, and bank-select/IRQ write protocol.
//! This module holds that shared core so individual boards only implement their
//! own bank-mapping and mirroring quirks.
//!
//! <https://wiki.nesdev.org/w/index.php/MMC3>

use crate::common::{Clock, Reset, ResetKind};
use serde::{Deserialize, Serialize};

/// MMC3 Revision.
///
/// See: <https://forums.nesdev.org/viewtopic.php?p=62546#p62546>
///
/// Known Revisions:
///
/// Conquest of the Crystal Palace (MMC3B S 9039 1 DB)
/// Kickle Cubicle (MMC3B S 9031 3 DA)
/// M.C. Kids (MMC3B S 9152 3 AB)
/// Mega Man 3 (MMC3B S 9046 1 DB)
/// Super Mario Bros. 3 (MMC3B S 9027 5 A)
/// Startropics (MMC6B P 03'5)
/// Batman (MMC3B 9006KP006)
/// Golgo 13: The Mafat Conspiracy (MMC3B 9016KP051)
/// Crystalis (MMC3B 9024KPO53)
/// Legacy of the Wizard (MMC3A 8940EP)
///
/// Only major difference is the IRQ counter
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    /// MMC3 Revision A
    A,
    /// MMC3 Revisions B & C
    #[default]
    BC,
    /// Acclaims MMC3 clone - clocks on falling edge
    Acc,
}

/// The `MMC3` register file: bank registers plus the scanline IRQ counter.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Mmc3 {
    pub revision: Revision,
    pub bank_select: u8,
    pub bank_values: [u8; 8],
    pub irq_latch: u8,
    pub irq_counter: u8,
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub irq_reload: bool,
    pub master_clock: u32,
    pub a12_low_clock: u32,
}

impl Mmc3 {
    pub const fn set_revision(&mut self, revision: Revision) {
        self.revision = revision;
    }

    /// `$8000`: select which bank register the next `$8001` write updates, plus
    /// the PRG/CHR mode bits (interpreted by the board).
    pub const fn write_bank_select(&mut self, val: u8) {
        self.bank_select = val;
    }

    /// `$8001`: write the value of the register selected by the low 3 bits of
    /// `bank_select` (standard MMC3 decoding).
    pub const fn write_bank_data(&mut self, val: u8) {
        self.bank_values[(self.bank_select & 0x07) as usize] = val;
    }

    /// `$C000`: set the IRQ counter reload latch.
    pub const fn write_irq_latch(&mut self, val: u8) {
        self.irq_latch = val;
    }

    /// `$C001`: schedule an IRQ counter reload on the next clock.
    pub const fn write_irq_reload(&mut self) {
        self.irq_reload = true;
    }

    /// `$E000`: disable IRQs and acknowledge any pending IRQ.
    pub const fn write_irq_disable(&mut self) {
        self.irq_enabled = false;
        self.irq_pending = false;
    }

    /// `$E001`: enable IRQs.
    pub const fn write_irq_enable(&mut self) {
        self.irq_enabled = true;
    }

    const fn is_a12_rising_edge(&mut self, addr: u16) -> bool {
        if addr & 0x1000 > 0 {
            // NOTE: This is technical 3 falling edges of M2 - but because the mapper doesn't have
            // direct access to the CPUs clock, and is clocked after the PPU runs and calls this
            // method, we're off by 1
            let is_rising_edge =
                self.a12_low_clock > 0 && self.master_clock.wrapping_sub(self.a12_low_clock) >= 4;
            self.a12_low_clock = 0;
            return is_rising_edge;
        } else if self.a12_low_clock == 0 {
            self.a12_low_clock = self.master_clock;
        }
        false
    }

    /// Clock the scanline IRQ counter on a PPU A12 rising edge.
    pub const fn clock_irq(&mut self, addr: u16) {
        if self.is_a12_rising_edge(addr) {
            let counter = self.irq_counter;
            if self.irq_counter == 0 || self.irq_reload {
                self.irq_counter = self.irq_latch;
            } else {
                self.irq_counter -= 1;
            }
            if matches!(self.revision, Revision::A) {
                if (counter > 0 || self.irq_reload) && self.irq_counter == 0 && self.irq_enabled {
                    self.irq_pending = true;
                }
            } else if self.irq_counter == 0 && self.irq_enabled {
                self.irq_pending = true;
            }
            self.irq_reload = false;
        }
    }

    /// Whether an IRQ is pending acknowledgement.
    #[must_use]
    pub const fn irq_pending(&self) -> bool {
        self.irq_pending
    }
}

impl Reset for Mmc3 {
    fn reset(&mut self, _kind: ResetKind) {
        // Preserve the configured revision; reset only the volatile register file.
        let revision = self.revision;
        *self = Self::default();
        self.revision = revision;
    }
}

impl Clock for Mmc3 {
    fn clock(&mut self) {
        self.master_clock = self.master_clock.wrapping_add(1);
    }
}
