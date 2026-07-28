//! OS-specific filesystem operations.

use crate::fs::{Error, Result};
use std::{
    fs::{File, create_dir_all, remove_dir_all},
    io::{Read, Write},
    path::Path,
};

/// Opens `path` for writing, creating parent directories as needed.
///
/// # Errors
///
/// If the directories or file cannot be created.
pub fn writer_impl(path: impl AsRef<Path>) -> Result<impl Write> {
    let path = path.as_ref();
    let Some(directory) = path.parent() else {
        return Err(Error::InvalidPath(path.to_path_buf()));
    };
    if !directory.exists() {
        create_dir_all(directory)
            .map_err(|err| Error::io(err, format!("failed to create directory {directory:?}")))?;
    }
    File::create(path)
        .map_err(|source| Error::io(source, format!("failed to create file {path:?}")))
}

/// Opens `path` for reading.
///
/// # Errors
///
/// If the file cannot be opened.
pub fn reader_impl(path: impl AsRef<Path>) -> Result<impl Read> {
    let path = path.as_ref();
    File::open(path).map_err(|source| Error::io(source, format!("failed to open file {path:?}")))
}

/// Removes a directory and everything in it.
///
/// # Errors
///
/// If the directory cannot be removed.
pub fn clear_dir_impl(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(());
    }
    remove_dir_all(path)
        .map_err(|source| Error::io(source, format!("failed to remove directory {path:?}")))
}

/// Whether `path` exists.
pub fn exists_impl(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    path.exists()
}
