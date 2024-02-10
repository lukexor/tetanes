/// Initialize logging.
#[cfg(target_arch = "wasm32")]
pub fn init() {
    #[cfg(feature = "console_log")]
    {
        use log::Level;
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init_with_level(if cfg!(debug_assertions) {
            Level::Debug
        } else {
            Level::Warn
        })
        .expect("valid console log");
    }
}

/// Initialize logging.
#[cfg(not(target_arch = "wasm32"))]
pub fn init() {
    use std::env;
    if env::var("RUST_LOG").is_err() {
        env::set_var(
            "RUST_LOG",
            if cfg!(debug_assertions) {
                "tetanes=debug"
            } else {
                "tetanes=warn"
            },
        );
    }

    pretty_env_logger::init_timed();
}
