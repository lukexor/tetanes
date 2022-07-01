use crate::common::Clock;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct LengthCounter {
    pub enabled: bool,
    pub counter: u8, // Entry into LENGTH_TABLE
}

impl LengthCounter {
    const LENGTH_TABLE: [u8; 32] = [
        10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96,
        22, 192, 24, 72, 26, 16, 28, 32, 30,
    ];

    pub const fn new() -> Self {
        Self {
            enabled: false,
            counter: 0u8,
        }
    }

    #[inline]
    #[must_use]
    pub const fn counter(&self) -> u8 {
        self.counter
    }

    #[inline]
    pub fn load_value(&mut self, val: u8) {
        self.counter = Self::LENGTH_TABLE[(val >> 3) as usize]; // D7..D3
    }

    #[inline]
    pub fn write_ctrl(&mut self, val: u8) {
        self.enabled = (val >> 5) & 1 == 0; // !D5
    }
}

impl Clock for LengthCounter {
    #[inline]
    fn clock(&mut self) -> usize {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
            1
        } else {
            0
        }
    }
}
