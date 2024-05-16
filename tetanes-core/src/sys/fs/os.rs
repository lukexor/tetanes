//! OS-specific filesystem operations.

use crate::fs::{Error, Result};
use std::{
    fs::{create_dir_all, remove_dir_all, File},
    io::{Read, Write},
    path::Path,
};

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

pub fn reader_impl(path: impl AsRef<Path>) -> Result<impl Read> {
    let path = path.as_ref();
    File::open(path).map_err(|source| Error::io(source, format!("failed to open file {path:?}")))
}

pub fn clear_dir_impl(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    remove_dir_all(path)
        .map_err(|source| Error::io(source, format!("failed to remove directory {path:?}")))
}
