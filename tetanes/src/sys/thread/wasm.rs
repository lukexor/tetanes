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

/// Blocks unless or until the current thread's token is made available or
/// the specified duration has been reached (may wake spuriously).
#[allow(clippy::missing_const_for_fn)]
pub fn park_timeout_impl(_dur: Duration) {}

/// Sleeps the current thread for the specified duration by yielding.
pub async fn sleep_impl(dur: Duration) {
    let mut cb = |resolve: Function, _reject: Function| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, dur.as_secs() as i32)
            .expect("Failed to call set_timeout");
    };
    JsFuture::from(Promise::new(&mut cb))
        .await
        .expect("failed to sleep");
}
