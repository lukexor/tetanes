use crate::sys::thread;
use std::future::Future;
use tetanes_core::time::Duration;

/// Spawn a future to be run until completion.
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    thread::spawn_impl(future);
}

/// Blocks unless or until the current thread's token is made available or
/// the specified duration has been reached (may wake spuriously).
pub fn park_timeout(dur: Duration) {
    thread::park_timeout_impl(dur);
}

/// Sleeps the current thread for the specified duration.
pub async fn sleep(dur: Duration) {
    thread::sleep_impl(dur).await
}
