use crate::sys::logging;
use std::env;
use tracing_subscriber::{
    filter::Targets,
    layer::{Layered, SubscriberExt},
    util::SubscriberInitExt,
    Registry,
};

fn create_registry() -> Layered<Targets, Registry> {
    let default_log = if cfg!(debug_assertions) {
        "warn,tetanes=debug,tetanes-core=debug"
    } else {
        "warn,tetanes=info,tetanes-core=info"
    };
    let default_filter = default_log.parse::<Targets>().unwrap_or_default();

    tracing_subscriber::registry().with(
        env::var("RUST_LOG")
            .ok()
            .and_then(|filter| filter.parse::<Targets>().ok())
            .unwrap_or(default_filter),
    )
}

/// Initialize logging.
pub fn init() -> anyhow::Result<logging::Log> {
    let (registry, log) = logging::init_impl(create_registry())?;
    if let Err(err) = registry.try_init() {
        anyhow::bail!("setting tracing default failed: {err:?}");
    }

    Ok(log)
}
