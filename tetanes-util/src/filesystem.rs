use crate::NesResult;
use anyhow::Context;
use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use std::{
    io::{Read, Write},
    path::Path,
};

const SAVE_FILE_MAGIC_LEN: usize = 8;
const SAVE_FILE_MAGIC: [u8; SAVE_FILE_MAGIC_LEN] = *b"TETANES\x1a";
// Keep this separate from Semver because breaking API changes may not invalidate the save format.
const SAVE_VERSION: &str = "1";

/// Writes a header including a magic string and a version
///
/// # Errors
///
/// If the header fails to write to disk, then an error is returned.
pub(crate) fn write_save_header(f: &mut impl Write) -> NesResult<()> {
    f.write_all(&SAVE_FILE_MAGIC)?;
    f.write_all(SAVE_VERSION.as_bytes())?;
    Ok(())
}

/// Verifies a `TetaNES` saved state header.
///
/// # Errors
///
/// If the header fails to validate, then an error is returned.
pub(crate) fn validate_save_header(f: &mut impl Read) -> NesResult<()> {
    use anyhow::anyhow;

    let mut magic = [0u8; SAVE_FILE_MAGIC_LEN];
    f.read_exact(&mut magic)?;
    if magic == SAVE_FILE_MAGIC {
        let mut version = [0u8];
        f.read_exact(&mut version)?;
        if version == SAVE_VERSION.as_bytes() {
            Ok(())
        } else {
            Err(anyhow!(
                "invalid save file version. current: {}, save file: {}",
                SAVE_VERSION,
                version[0],
            ))
        }
    } else {
        Err(anyhow!("invalid save file format"))
    }
}

pub fn encode_data(data: &[u8]) -> NesResult<Vec<u8>> {
    let mut encoded = vec![];
    let mut encoder = DeflateEncoder::new(&mut encoded, Compression::default());
    encoder.write_all(data).context("failed to encode data")?;
    encoder.finish().context("failed to write data")?;
    Ok(encoded)
}

pub fn decode_data(data: &[u8]) -> NesResult<Vec<u8>> {
    let mut decoded = vec![];
    let mut decoder = DeflateDecoder::new(data);
    decoder
        .read_to_end(&mut decoded)
        .context("failed to read data")?;
    Ok(decoded)
}

#[cfg(target_arch = "wasm32")]
pub fn save_data(_path: impl AsRef<Path>, _data: &[u8]) -> NesResult<()> {
    // TODO: provide file download?
    Err(anyhow::anyhow!("not implemented"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_data(path: impl AsRef<Path>, data: &[u8]) -> NesResult<()> {
    let path = path.as_ref();
    let directory = path.parent().expect("can not save to root path");
    if !directory.exists() {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("failed to create directory {directory:?}"))?;
    }
    let write_data = || {
        let mut writer = std::fs::File::create(path)
            .with_context(|| format!("failed to create file {path:?}"))?;
        save_writer(&mut writer, data)
    };
    if path.exists() {
        // Check if exists and header is different, so we avoid overwriting
        let mut reader =
            std::fs::File::open(path).with_context(|| format!("failed to open file {path:?}"))?;
        validate_save_header(&mut reader)
            .with_context(|| format!("failed to validate header {path:?}"))
            .and_then(|_| write_data())
    } else {
        write_data()
    }
}

pub fn save_writer(writer: &mut impl Write, data: &[u8]) -> NesResult<()> {
    write_save_header(writer).context("failed to write header")?;
    let mut encoder = DeflateEncoder::new(writer, Compression::default());
    encoder.write_all(data).context("failed to write file")?;
    encoder.finish().context("failed to encode file")?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn load_data(_path: impl AsRef<Path>) -> NesResult<Vec<u8>> {
    // TODO: provide file upload?
    Err(anyhow::anyhow!("not implemented"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_data(path: impl AsRef<Path>) -> NesResult<Vec<u8>> {
    let path = path.as_ref();
    let mut reader =
        std::fs::File::open(path).with_context(|| format!("Failed to open file {path:?}"))?;
    load_reader(&mut reader)
}

pub fn load_reader(reader: &mut impl Read) -> NesResult<Vec<u8>> {
    let mut bytes = vec![];
    // Don't care about the size read
    let _ = validate_save_header(reader)
        .context("failed to validate header")
        .and_then(|_| {
            let mut decoder = DeflateDecoder::new(reader);
            decoder
                .read_to_end(&mut bytes)
                .context("failed to decode file")
        })?;
    Ok(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn filename(path: &Path) -> &str {
    use tracing::warn;

    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_else(|| {
            warn!("invalid rom_path: {path:?}");
            "??"
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_header() {
        let mut file = Vec::new();
        assert!(write_save_header(&mut file).is_ok(), "write save header");
        assert!(
            validate_save_header(&mut file.as_slice()).is_ok(),
            "validate save header"
        );
    }
}
