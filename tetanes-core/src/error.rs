//! Deprecated crate-wide error type.
//!
//! Nothing in the crate produces or consumes these - each module carries its own `Error`/`Result`
//! pair instead ([`cart`](crate::cart), [`fs`](crate::fs), [`genie`](crate::genie),
//! [`mapper`](crate::mapper), [`control_deck`](crate::control_deck)), and
//! [`control_deck::Error`](crate::control_deck::Error) is what [`ControlDeck`] methods return.
//!
//! [`ControlDeck`]: crate::control_deck::ControlDeck

// The type is deprecated but its own definition, `Result` alias and inherent impl still mention it.
#![allow(deprecated)]

use std::path::PathBuf;
use thiserror::Error;

/// Result alias for the deprecated crate-wide [`Error`](enum@Error).
#[deprecated(
    since = "0.15.0",
    note = "use the per-module `Result`, e.g. `control_deck::Result`"
)]
pub type Result<T> = std::result::Result<T, Error>;

/// Deprecated crate-wide error type. See the [module docs](self).
#[derive(Error, Debug)]
#[must_use]
#[deprecated(
    since = "0.15.0",
    note = "use the per-module `Error`, e.g. `control_deck::Error`"
)]
pub enum Error {
    /// A save file's version does not match this build's.
    #[error("invalid save version (expected {expected:?}, found: {found:?})")]
    InvalidSaveVersion {
        /// Version this build writes.
        expected: &'static str,
        /// Version found in the file.
        found: String,
    },
    /// A save file's magic header is missing or malformed.
    #[error("invalid tetanes header (path: {path:?}. {error}")]
    InvalidSaveHeader {
        /// File the header was read from.
        path: PathBuf,
        /// What was wrong with it.
        error: String,
    },
    /// A configuration field was given an unusable value.
    #[error("invalid configuration {value:?} for {field:?}")]
    InvalidConfig {
        /// Name of the offending field.
        field: &'static str,
        /// Value it was given.
        value: String,
    },
    /// Filesystem error, with the operation that caused it.
    #[error("{context}: {source:?}")]
    Io {
        /// What was being attempted.
        context: String,
        /// The underlying error.
        source: std::io::Error,
    },
    /// Any other error.
    #[error("{0}")]
    Unknown(String),
}

impl Error {
    /// Creates an [`Error::Io`] from `source`, describing what was being attempted as `context`.
    pub fn io(source: std::io::Error, context: impl Into<String>) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
