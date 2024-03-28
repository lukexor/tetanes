use serde::{Deserialize, Serialize};

/// APU Sweep provides frequency sweeping for the APU pulse channels.
///
/// See: <https://www.nesdev.org/wiki/APU_Sweep>
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Sweep {
    pub enabled: bool,
    pub reload: bool,
    pub negate: bool, // Treats PulseChannel 1 differently than PulseChannel 2
    pub timer: u8,    // counter reload value
    pub counter: u8,  // current timer value
    pub shift: u8,
}
