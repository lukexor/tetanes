use crate::sys::fs;
use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;
use tracing::warn;

const SAVE_FILE_MAGIC_LEN: usize = 8;
const SAVE_FILE_MAGIC: [u8; SAVE_FILE_MAGIC_LEN] = *b"TETANES\x1a";
// Keep this separate from Semver because breaking API changes may not invalidate the save format.
const SAVE_VERSION: &str = "1";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[must_use]
pub enum Error {
    #[error("invalid tetanes header: {0}")]
    InvalidHeader(String),
    #[error("failed to write tetanes header: {0:?}")]
    WriteHeaderFailed(std::io::Error),
    #[error("failed to encode data: {0:?}")]
    EncodingFailed(std::io::Error),
    #[error("failed to decode data: {0:?}")]
    DecodingFailed(std::io::Error),
    #[error("failed to serialize data: {0:?}")]
    SerializationFailed(String),
    #[error("invalid path: {0:?}")]
    InvalidPath(PathBuf),
    #[error("{context}: {source:?}")]
    Io {
        source: std::io::Error,
        context: String,
    },
    #[error("{0}")]
    Custom(String),
}

impl Error {
    pub fn io(source: std::io::Error, context: impl Into<String>) -> Self {
        Self::Io {
            source,
            context: context.into(),
        }
    }

    pub fn custom(error: impl Into<String>) -> Self {
        Self::Custom(error.into())
    }
}

/// Writes a header including a magic string and a version
///
/// # Errors
///
/// If the header fails to write to disk, then an error is returned.
pub(crate) fn write_header(f: &mut impl Write) -> std::io::Result<()> {
    f.write_all(&SAVE_FILE_MAGIC)?;
    f.write_all(SAVE_VERSION.as_bytes())
}

/// Verifies a `TetaNES` saved state header.
///
/// # Errors
///
/// If the header fails to validate, then an error is returned.
pub(crate) fn validate_header(f: &mut impl Read) -> Result<()> {
    let mut magic = [0u8; SAVE_FILE_MAGIC_LEN];
    f.read_exact(&mut magic)
        .map_err(|s| Error::InvalidHeader(s.to_string()))?;
    if magic != SAVE_FILE_MAGIC {
        return Err(Error::InvalidHeader(format!(
            "invalid magic (expected {SAVE_FILE_MAGIC:?}, found: {magic:?}",
        )));
    }

    let mut version = [0u8];
    f.read_exact(&mut version)
        .map_err(|s| Error::InvalidHeader(s.to_string()))?;
    if version == SAVE_VERSION.as_bytes() {
        Ok(())
    } else {
        Err(Error::InvalidHeader(format!(
            "invalid version (expected {SAVE_VERSION:?}, found: {version:?}",
        )))
    }
}

pub fn encode(mut writer: &mut impl Write, data: &[u8]) -> std::io::Result<()> {
    let mut encoder = DeflateEncoder::new(&mut writer, Compression::default());
    encoder.write_all(data)?;
    encoder.finish()?;
    Ok(())
}

pub fn decode(data: impl Read) -> std::io::Result<Vec<u8>> {
    let mut decoded = vec![];
    let mut decoder = DeflateDecoder::new(data);
    decoder.read_to_end(&mut decoded)?;
    Ok(decoded)
}

pub fn save<T>(path: impl AsRef<Path>, value: &T) -> Result<()>
where
    T: ?Sized + Serialize,
{
    let data =
        bincode::serialize(value).map_err(|err| Error::SerializationFailed(err.to_string()))?;
    let mut writer = fs::writer_impl(path)?;
    write_header(&mut writer).map_err(Error::WriteHeaderFailed)?;
    encode(&mut writer, &data).map_err(Error::EncodingFailed)?;
    Ok(())
}

pub fn save_raw(path: impl AsRef<Path>, value: &[u8]) -> Result<()> {
    let mut writer = fs::writer_impl(path)?;
    writer
        .write_all(value)
        .map_err(|err| Error::io(err, "failed to save data"))?;
    Ok(())
}

pub fn load<T>(path: impl AsRef<Path>) -> Result<T>
where
    T: DeserializeOwned,
{
    let mut reader = fs::reader_impl(path)?;
    validate_header(&mut reader)?;
    let data = decode(&mut reader).map_err(Error::DecodingFailed)?;
    bincode::deserialize(&data).map_err(|err| Error::SerializationFailed(err.to_string()))
}

pub fn load_raw(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let mut reader = fs::reader_impl(path)?;
    let mut data = vec![];
    reader
        .read_to_end(&mut data)
        .map_err(|err| Error::io(err, "failed to load data"))?;
    Ok(data)
}

pub fn filename(path: &Path) -> &str {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_else(|| {
            warn!("invalid path without file_name: {path:?}");
            "??"
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_header() {
        let mut file = Vec::new();
        assert!(write_header(&mut file).is_ok(), "write header");
        assert!(
            validate_header(&mut file.as_slice()).is_ok(),
            "validate header"
        );
    }
}
