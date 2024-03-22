use crate::sys::thread;
use std::future::Future;
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
