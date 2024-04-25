use std::{future::Future, thread};
use tetanes_core::time::{Duration, Instant};

/// Spawn a future to be run until completion.
pub fn spawn_impl<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    pollster::block_on(future)
}

/// Blocks unless or until the current thread's token is made available or
/// the specified duration has been reached (may wake spuriously).
pub fn park_timeout_impl(dur: Duration) {
    let beginning_park = Instant::now();
    let mut timeout_remaining = dur;
    loop {
        thread::park_timeout(timeout_remaining);
        let elapsed = beginning_park.elapsed();
        if elapsed >= dur {
            break;
        }
        timeout_remaining = dur - elapsed;
    }
}
