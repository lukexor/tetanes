use std::env;
use tracing_subscriber::{
    filter::Targets,
    fmt,
    layer::{Layered, SubscriberExt},
    util::SubscriberInitExt,
    Registry,
};

fn create_registry() -> Layered<Targets, Registry> {
    let default_filter = if cfg!(debug_assertions) {
        "tetanes=debug,tetanes-core=debug,tetanes-util=debug"
    } else {
        "tetanes=info,tetanes-core=info,tetanes-util=info"
    }
    .parse::<Targets>()
    .expect("valid filter");
    let filter = match env::var("RUST_LOG") {
        Ok(filter) => filter.parse::<Targets>().unwrap_or(default_filter),
        Err(_) => default_filter,
    };

    tracing_subscriber::registry().with(filter)
}

#[derive(Debug)]
#[must_use]
pub struct Guard {
    #[cfg(target_arch = "wasm32")]
    _inner: (),
    #[cfg(not(target_arch = "wasm32"))]
    _inner: tracing_appender::non_blocking::WorkerGuard,
}

/// Initialize logging.
#[cfg(target_arch = "wasm32")]
pub fn init() -> Guard {
    use tracing_subscriber::fmt::format::Pretty;
    use tracing_web::{performance_layer, MakeWebConsoleWriter};

    #[cfg(debug_assertions)]
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    let console_layer = fmt::layer()
        .compact()
        .with_line_number(true)
        .with_ansi(false)
        .without_time()
        .with_writer(MakeWebConsoleWriter::new());
    let perf_layer = performance_layer().with_details_from_fields(Pretty::default());

    if let Err(err) = create_registry()
        .with(console_layer)
        .with(perf_layer)
        .try_init()
    {
        eprintln!("initializing tracing failed: {err:?}");
    }

    Guard { _inner: () }
}

/// Initialize logging.
#[cfg(not(target_arch = "wasm32"))]
pub fn init() -> Guard {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};

    const LOG_DIR: &str = "logs";
    const LOG_PREFIX: &str = "tetanes.log";

    let registry = create_registry();

    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .max_log_files(3)
        .filename_prefix(LOG_PREFIX)
        .build(LOG_DIR)
        .expect("Failed to create log file");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    let registry = registry.with(
        fmt::layer()
            .compact()
            .with_line_number(true)
            .with_writer(file_writer),
    );

    #[cfg(debug_assertions)]
    let registry = registry.with(
        fmt::layer()
            .compact()
            .with_line_number(true)
            .with_writer(std::io::stderr),
    );

    if let Err(err) = registry.try_init() {
        eprintln!("setting tracing default failed: {err:?}");
    }

    Guard { _inner: guard }
}
