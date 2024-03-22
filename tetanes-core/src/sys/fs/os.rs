use crate::fs::{self, Error, Result};
use std::{
    fs::{create_dir_all, File},
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
    let create_file = || {
        File::create(path)
            .map_err(|source| Error::io(source, format!("failed to create file {path:?}")))
    };
    if path.exists() {
        // Check if exists and header is different, so we avoid overwriting
        let mut reader = File::open(path)
            .map_err(|source| Error::io(source, format!("failed to open file {path:?}")))?;
        fs::validate_header(&mut reader).and_then(|_| create_file())
    } else {
        create_file()
    }
}

pub fn reader_impl(path: impl AsRef<Path>) -> Result<impl Read> {
    let path = path.as_ref();
    File::open(path).map_err(|source| Error::io(source, format!("failed to open file {path:?}")))
}
