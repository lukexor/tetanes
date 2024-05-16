use crate::sys::logging;
use std::env;
use tracing_subscriber::{
    filter::Targets,
    layer::{Layered, SubscriberExt},
    util::SubscriberInitExt,
    Registry,
};

fn create_registry() -> Layered<Targets, Registry> {
    let default_filter = if cfg!(debug_assertions) {
        "warn,tetanes=debug,tetanes-core=debug"
    } else {
        "warn,tetanes=info,tetanes-core=info"
    }
    .parse::<Targets>()
    .expect("valid filter");

    tracing_subscriber::registry().with(
        env::var("RUST_LOG")
            .ok()
            .and_then(|filter| filter.parse::<Targets>().ok())
            .unwrap_or(default_filter),
    )
}

/// Initialize logging.
pub fn init() -> logging::Log {
    let (registry, log) = logging::init_impl(create_registry());
    if let Err(err) = registry.try_init() {
        eprintln!("setting tracing default failed: {err:?}");
    }
    log
}
