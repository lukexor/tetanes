use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod wasm;
        pub(crate) use wasm::*;
    } else {
        mod os;
        pub(crate) use os::*;
    }
}
