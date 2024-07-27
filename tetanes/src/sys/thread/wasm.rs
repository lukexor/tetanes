use std::future::Future;
use tetanes_core::time::Duration;
use wasm_bindgen_futures::JsFuture;
use web_sys::js_sys::{Function, Promise};

/// Spawn a future to be run until completion.
pub fn spawn_impl<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

/// Blocking, and thus parking is not allowed in wasm.
#[allow(clippy::missing_const_for_fn)]
pub fn park_timeout_impl(_dur: Duration) {}

/// Sleeps the current thread for the specified duration.
pub async fn sleep_impl(dur: Duration) {
    let mut cb = |resolve: Function, _reject: Function| {
        if let Some(window) = web_sys::window() {
            if let Err(err) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                dur.as_secs() as i32,
            ) {
                tracing::error!("failed to call window.set_timeout: {err:?}");
            }
        }
    };
    if let Err(err) = JsFuture::from(Promise::new(&mut cb)).await {
        tracing::error!("failed to create sleep future: {err:?}");
    }
}
