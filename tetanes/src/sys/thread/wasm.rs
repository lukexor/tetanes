use std::future::Future;
use tetanes_core::time::Duration;

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
