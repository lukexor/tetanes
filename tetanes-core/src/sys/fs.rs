//! Platform-specific filesystem methods.

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod wasm;
        pub use wasm::*;
    } else {
        mod os;
        pub use os::*;
    }
}
