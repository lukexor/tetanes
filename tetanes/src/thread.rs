use crate::sys::thread;
use std::future::Future;
use tetanes_core::time::Duration;
use tracing::error;

/// Spawn a future to be run until completion.
pub fn spawn<F>(future: F)
where
    F: Future<Output = anyhow::Result<()>> + 'static,
{
    thread::spawn_impl(async {
        if let Err(err) = future.await {
            error!("spawned future failed: {err:?}");
        }
    })
}

/// Blocks unless or until the current thread's token is made available or
/// the specified duration has been reached (may wake spuriously).
pub fn park_timeout(dur: Duration) {
    thread::park_timeout_impl(dur);
}
