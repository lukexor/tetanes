//! Platform-specific time and date methods.

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        pub use web_time::{Duration, Instant};
    } else {
        pub use std::time::{Duration, Instant};
    }
}
