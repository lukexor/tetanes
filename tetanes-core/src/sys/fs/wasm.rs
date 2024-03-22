use crate::fs::{Error, Result};
use std::{
    io::{Empty, Read, Write},
    path::Path,
};

pub fn writer_impl(_path: impl AsRef<Path>) -> Result<impl Write> {
    // TODO: provide file download
    Err::<Empty, _>(Error::custom("not implemented: wasm write"))
}

pub fn reader_impl(_path: impl AsRef<Path>) -> Result<impl Read> {
    // TODO: provide file upload?
    Err::<Empty, _>(Error::custom("not implemented: wasm read"))
}
