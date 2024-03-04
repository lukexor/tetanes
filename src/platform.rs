pub mod time {
    #[cfg(not(target_arch = "wasm32"))]
    pub use std::time::{Duration, Instant};
    #[cfg(target_arch = "wasm32")]
    pub use web_time::{Duration, Instant};
}

pub mod thread {
    use crate::NesResult;
    use std::future::Future;
    use tracing::error;

    /// Spawn a future to be run until completion.
    pub fn spawn<F>(future: F) -> NesResult<()>
    where
        F: Future<Output = NesResult<()>> + 'static,
    {
        let execute = async {
            if let Err(err) = future.await {
                error!("spawned future failed: {err:?}");
            }
        };

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(execute);

        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(execute);

        Ok(())
    }
}
