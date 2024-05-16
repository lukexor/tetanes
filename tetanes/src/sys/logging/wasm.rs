use tracing_subscriber::{
    fmt::{self, format::Pretty},
    layer::SubscriberExt,
    registry::LookupSpan,
    util::SubscriberInitExt,
};
use tracing_web::{performance_layer, MakeWebConsoleWriter};

pub struct Log;

pub fn init_impl<S>(registry: S) -> (impl SubscriberInitExt, Log)
where
    S: SubscriberExt + for<'a> LookupSpan<'a> + Sync + Send,
{
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    let console_layer = fmt::layer()
        .compact()
        .with_line_number(true)
        .with_ansi(false)
        .without_time()
        .with_writer(MakeWebConsoleWriter::new());
    let perf_layer = performance_layer().with_details_from_fields(Pretty::default());
    let registry = registry.with(console_layer).with(perf_layer);
    (registry, Log)
}
