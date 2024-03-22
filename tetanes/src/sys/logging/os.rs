
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{
    fmt, layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt,
};

const LOG_DIR: &str = "logs";
const LOG_PREFIX: &str = "tetanes.log";

#[must_use]
pub struct Log {
    _guard: WorkerGuard,
}

pub fn init_impl<S>(registry: S) -> (impl SubscriberInitExt, Log)
where
    S: SubscriberExt + for<'a> LookupSpan<'a> + Sync + Send,
{
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .max_log_files(3)
        .filename_prefix(LOG_PREFIX)
        .build(LOG_DIR)
        .expect("Failed to create log file");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let registry = registry
        .with(
            fmt::layer()
                .compact()
                .with_line_number(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_writer(file_writer),
        )
        .with(
            fmt::layer()
                .compact()
                .with_line_number(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_writer(std::io::stderr),
        );

    (registry, Log { _guard: guard })
}
