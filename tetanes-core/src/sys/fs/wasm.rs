//! Web-specific filesystem operations.

use crate::fs::{Error, Result};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use std::{
    io::{self, Read, Write},
    mem,
    path::{Path, PathBuf},
};
use web_sys::js_sys;

/// Encode bytes for local storage, which holds strings rather than bytes.
///
/// Base64 costs 4 characters per 3 bytes. The obvious alternative - one character per byte - is
/// smaller on paper but relies on how the browser stores code points above 0x7F, where base64's
/// alphabet is plain ASCII whatever the engine does internally.
fn encode(data: &[u8]) -> String {
    BASE64.encode(data)
}

/// Decode a local-storage value written by [`encode`].
///
/// Entries written before the switch to base64 are JSON arrays of decimal byte values, which cost
/// 3-4 characters per byte. They are still read, so an existing save state or SRAM survives the
/// change; `[` cannot begin a base64 string, so the two are told apart by their first byte rather
/// than by a version marker. Rewritten in the new form the next time they are saved.
fn decode(value: &str) -> Result<Vec<u8>> {
    if value.starts_with('[') {
        return serde_json::from_str(value).map_err(|err| {
            tracing::error!("failed to deserialize legacy json data: {err:?}");
            Error::custom("failed to deserialize data")
        });
    }
    BASE64.decode(value).map_err(|err| {
        tracing::error!("failed to decode data: {err:?}");
        Error::custom("failed to deserialize data")
    })
}

/// A [`Write`] that buffers everything and commits it to local storage on drop, since local
/// storage takes a whole value per key rather than a stream.
#[derive(Debug)]
#[must_use]
pub struct StoreWriter {
    path: PathBuf,
    data: Vec<u8>,
}

/// A [`Read`] over one local-storage entry, read out in full up front.
pub struct StoreReader {
    cursor: io::Cursor<Vec<u8>>,
}

/// The window's local storage, which is what stands in for a filesystem on the web.
///
/// # Errors
///
/// If there is no window, or storage is unavailable - private browsing modes refuse it.
pub fn local_storage() -> Result<web_sys::Storage> {
    let window = web_sys::window().ok_or_else(|| Error::custom("failed to get js window"))?;
    window
        .local_storage()
        .map_err(|err| {
            tracing::error!("failed to get local storage: {err:?}");
            Error::custom("failed to get storage")
        })?
        .ok_or_else(|| Error::custom("no storage available"))
}

impl Write for StoreWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.data.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let local_storage = local_storage().map_err(io::Error::other)?;

        let key = self.path.to_string_lossy();
        let data = mem::take(&mut self.data);
        let value = encode(&data);

        if let Err(err) = local_storage.set_item(&key, &value) {
            self.data = data;
            tracing::error!("failed to store data in local storage: {err:?}");
            return Err(io::Error::other("failed to write data"));
        }

        Ok(())
    }
}

impl Read for StoreReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buf)
    }
}

/// Opens `path` for writing, as a local-storage key.
///
/// # Errors
///
/// Infallible today; the `Result` matches the native backend's signature.
pub fn writer_impl(path: impl AsRef<Path>) -> Result<impl Write> {
    let path = path.as_ref();
    Ok(StoreWriter {
        path: path.to_path_buf(),
        data: Vec::new(),
    })
}

/// Opens `path` for reading, from a local-storage key. A missing key reads as empty.
///
/// # Errors
///
/// If storage is unavailable, or the entry is neither base64 nor the JSON array older entries were
/// written as.
pub fn reader_impl(path: impl AsRef<Path>) -> Result<impl Read> {
    let path = path.as_ref();
    let local_storage = local_storage()?;

    let key = path.to_string_lossy().into_owned();
    let data = local_storage
        .get_item(&key)
        .map_err(|_| Error::custom("failed to find data for {key}"))?
        .map(|value| decode(&value))
        .unwrap_or_else(|| Ok(Vec::new()))?;

    Ok(StoreReader {
        cursor: io::Cursor::new(data),
    })
}

/// Removes every local-storage key beginning with `path`, local storage having no directories.
///
/// # Errors
///
/// If storage is unavailable.
pub fn clear_dir_impl(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref().to_string_lossy();
    let local_storage = local_storage()?;

    for key in js_sys::Object::keys(&local_storage)
        .iter()
        .filter_map(|key| key.as_string())
        .filter(|key| key.starts_with(&*path))
    {
        let _ = local_storage.remove_item(&key);
    }

    Ok(())
}

/// Whether a local-storage entry exists for `path`.
pub fn exists_impl(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    let Ok(local_storage) = local_storage() else {
        return false;
    };

    let key = path.to_string_lossy();
    matches!(local_storage.get_item(&key), Ok(Some(_)))
}
