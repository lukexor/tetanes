//! Error handling.

use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[must_use]
pub enum Error {
    #[error("invalid save version (expected {expected:?}, found: {found:?})")]
    InvalidSaveVersion {
        expected: &'static str,
        found: String,
    },
    #[error("invalid tetanes header (path: {path:?}. {error}")]
    InvalidSaveHeader { path: PathBuf, error: String },
    #[error("invalid configuration {value:?} for {field:?}")]
    InvalidConfig { field: &'static str, value: String },
    #[error("{context}: {source:?}")]
    Io {
        context: String,
        source: std::io::Error,
    },
    #[error("{0}")]
    Unknown(String),
}

impl Error {
    pub fn io(source: std::io::Error, context: impl Into<String>) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
