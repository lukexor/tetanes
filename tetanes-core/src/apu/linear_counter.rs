use serde::{Deserialize, Serialize};

/// APU Linear Counter provides duration control for the APU triangle channel.
///
/// See: <https://www.nesdev.org/wiki/APU_Triangle>
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct LinearCounter {
    pub reload: bool,
    pub control: bool,
    pub load: u8,
    pub counter: u8,
}

impl LinearCounter {
    pub const fn new() -> Self {
        Self {
            reload: false,
            control: false,
            load: 0u8,
            counter: 0u8,
        }
    }

    pub fn load_value(&mut self, val: u8) {
        self.load = val & 0x7F; // D6..D0
    }
}
