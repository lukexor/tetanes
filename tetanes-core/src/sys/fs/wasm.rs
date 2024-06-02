//! Web-specific filesystem operations.

use crate::fs::{Error, Result};
use std::{
    io::{self, Read, Write},
    mem,
    path::{Path, PathBuf},
};
use web_sys::js_sys;

#[derive(Debug)]
#[must_use]
pub struct StoreWriter {
    path: PathBuf,
    data: Vec<u8>,
}

pub struct StoreReader {
    cursor: io::Cursor<Vec<u8>>,
}

fn local_storage() -> Result<web_sys::Storage> {
    let window = web_sys::window().ok_or_else(|| Error::custom("failed to get js window"))?;
    window
        .local_storage()
        .map_err(|err| {
            tracing::error!("failed to get local storage: {err:?}");
            Error::custom(format!("failed to get storage"))
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
        let value = match serde_json::to_string(&data) {
            Ok(value) => value,
            Err(err) => {
                self.data = data;
                tracing::error!("failed to serialize data: {err:?}");
                return Err(io::Error::other("failed to serialize data"));
            }
        };

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

pub fn writer_impl(path: impl AsRef<Path>) -> Result<impl Write> {
    let path = path.as_ref();
    Ok(StoreWriter {
        path: path.to_path_buf(),
        data: Vec::new(),
    })
}

pub fn reader_impl(path: impl AsRef<Path>) -> Result<impl Read> {
    let path = path.as_ref();
    let local_storage = local_storage()?;

    let key = path.to_string_lossy().into_owned();
    let data = local_storage
        .get_item(&key)
        .map_err(|_| Error::custom("failed to find data for {key}"))?
        .map(|value| {
            serde_json::from_str(&value).map_err(|err| {
                tracing::error!("failed to deserialize data: {err:?}");
                Error::custom("failed to deserialize data")
            })
        })
        .unwrap_or_else(|| Ok(Vec::new()))?;

    Ok(StoreReader {
        cursor: io::Cursor::new(data),
    })
}

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

pub fn exists_impl(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    let Ok(local_storage) = local_storage() else {
        return false;
    };

    let key = path.to_string_lossy();
    matches!(local_storage.get_item(&key), Ok(Some(_)))
}
