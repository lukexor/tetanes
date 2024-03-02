use std::env;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{filter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const LOG_DIR: &str = "logs";
const LOG_PREFIX: &str = "tetanes.log";

/// Initialize logging.
pub fn init() -> WorkerGuard {
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    let default_filter = if cfg!(debug_assertions) {
        "tetanes=debug"
    } else {
        "tetanes=info"
    }
    .parse::<filter::Targets>()
    .expect("valid filter");
    let filter = match env::var("RUST_LOG") {
        Ok(filter) => filter.parse::<filter::Targets>().unwrap_or(default_filter),
        Err(_) => default_filter,
    };

    let registry = tracing_subscriber::registry().with(filter);
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .max_log_files(3)
        .filename_prefix(LOG_PREFIX)
        .build(LOG_DIR)
        .expect("Failed to create log file");
    let (non_blocking_file, file_log_guard) = tracing_appender::non_blocking(file_appender);
    let registry = registry.with(
        fmt::Layer::new()
            .compact()
            .with_line_number(true)
            .with_writer(non_blocking_file),
    );

    #[cfg(debug_assertions)]
    let registry = registry.with(
        fmt::Layer::new()
            .compact()
            .with_line_number(true)
            .with_writer(std::io::stderr),
    );

    if let Err(err) = registry.try_init() {
        eprintln!("setting tracing default failed: {err:?}");
    }

    file_log_guard
}
