pub type Error = anyhow::Error;
pub type Result<T, E = Error> = anyhow::Result<T, E>;

pub use Error as NesError;
pub use Result as NesResult;
