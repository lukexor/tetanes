use std::panic;
use tracing_subscriber::{
    fmt::{self, format::Pretty},
    layer::SubscriberExt,
    registry::LookupSpan,
    util::SubscriberInitExt,
};
use tracing_web::{MakeWebConsoleWriter, performance_layer};

pub struct Log;

pub(crate) fn init_impl<S>(registry: S) -> anyhow::Result<(impl SubscriberInitExt, Log)>
where
    S: SubscriberExt + for<'a> LookupSpan<'a> + Sync + Send,
{
    panic::set_hook(Box::new(|info: &panic::PanicHookInfo<'_>| {
        let error_div = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("error"));
        if let Some(error_div) = error_div
            && let Err(err) = error_div.class_list().remove_1("hidden")
        {
            tracing::error!("{err:?}")
        }

        console_error_panic_hook::hook(info);
    }));

    let console_layer = fmt::layer()
        .compact()
        .with_line_number(true)
        .with_ansi(false)
        .without_time() // Not available in wasm
        .with_writer(MakeWebConsoleWriter::new());
    let perf_layer = performance_layer().with_details_from_fields(Pretty::default());
    let registry = registry.with(console_layer).with(perf_layer);

    Ok((registry, Log))
}
