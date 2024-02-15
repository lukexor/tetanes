use super::NesResult;
use anyhow::Context;
use flate2::{bufread::DeflateDecoder, write::DeflateEncoder, Compression};
use std::{
    io::{BufReader, Read, Write},
    path::Path,
};

const SAVE_FILE_MAGIC_LEN: usize = 8;
const SAVE_FILE_MAGIC: [u8; SAVE_FILE_MAGIC_LEN] = *b"TETANES\x1a";
const MAJOR_VERSION: &str = env!("CARGO_PKG_VERSION_MAJOR");

/// Writes a header including a magic string and a version
///
/// # Errors
///
/// If the header fails to write to disk, then an error is returned.
pub(crate) fn write_save_header<F: Write>(f: &mut F) -> NesResult<()> {
    f.write_all(&SAVE_FILE_MAGIC)?;
    f.write_all(MAJOR_VERSION.as_bytes())?;
    Ok(())
}

/// Verifies a `TetaNES` saved state header.
///
/// # Errors
///
/// If the header fails to validate, then an error is returned.
pub(crate) fn validate_save_header<F: Read>(f: &mut F) -> NesResult<()> {
    use anyhow::anyhow;

    let mut magic = [0u8; SAVE_FILE_MAGIC_LEN];
    f.read_exact(&mut magic)?;
    if magic == SAVE_FILE_MAGIC {
        let mut version = [0u8];
        f.read_exact(&mut version)?;
        if version == MAJOR_VERSION.as_bytes() {
            Ok(())
        } else {
            Err(anyhow!(
                "invalid save file version. current: {}, save file: {}",
                MAJOR_VERSION,
                version[0],
            ))
        }
    } else {
        Err(anyhow!("invalid save file format"))
    }
}

pub(crate) fn encode_data(data: &[u8]) -> NesResult<Vec<u8>> {
    let mut encoded = vec![];
    let mut encoder = DeflateEncoder::new(&mut encoded, Compression::default());
    encoder.write_all(data).context("failed to encode data")?;
    encoder.finish().context("failed to write data")?;
    Ok(encoded)
}

pub(crate) fn decode_data(data: &[u8]) -> NesResult<Vec<u8>> {
    let mut decoded = vec![];
    let mut decoder = DeflateDecoder::new(BufReader::new(data));
    decoder
        .read_to_end(&mut decoded)
        .context("failed to read data")?;
    Ok(decoded)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn save_data<P>(_path: P, _data: &[u8]) -> NesResult<()>
where
    P: AsRef<Path>,
{
    // TODO: provide file download?
    anyhow::bail!("not implemented")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_data<P>(path: P, data: &[u8]) -> NesResult<()>
where
    P: AsRef<Path>,
{
    use std::io::BufWriter;

    let path = path.as_ref();
    let directory = path.parent().expect("can not save to root path");
    if !directory.exists() {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("failed to create directory {directory:?}"))?;
    }

    let write_data = || {
        let mut writer = BufWriter::new(
            std::fs::File::create(path)
                .with_context(|| format!("failed to create file {path:?}"))?,
        );
        write_save_header(&mut writer)
            .with_context(|| format!("failed to write header {path:?}"))?;
        let mut encoder = DeflateEncoder::new(writer, Compression::default());
        encoder
            .write_all(data)
            .with_context(|| format!("failed to encode file {path:?}"))?;
        encoder
            .finish()
            .with_context(|| format!("failed to write file {path:?}"))?;
        Ok(())
    };

    if path.exists() {
        // Check if exists and header is different, so we avoid overwriting
        let mut reader = BufReader::new(
            std::fs::File::open(path).with_context(|| format!("failed to open file {path:?}"))?,
        );
        validate_save_header(&mut reader)
            .with_context(|| format!("failed to validate header {path:?}"))
            .and_then(|_| write_data())?;
    } else {
        write_data()?;
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn load_data<P>(_path: P) -> NesResult<Vec<u8>>
where
    P: AsRef<Path>,
{
    // TODO: provide file upload?
    anyhow::bail!("not implemented")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load_data<P>(path: P) -> NesResult<Vec<u8>>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let mut reader = BufReader::new(
        std::fs::File::open(path).with_context(|| format!("Failed to open file {path:?}"))?,
    );
    let mut bytes = vec![];
    // Don't care about the size read
    let _ = validate_save_header(&mut reader)
        .with_context(|| format!("failed to validate header {path:?}"))
        .and_then(|_| {
            let mut decoder = DeflateDecoder::new(reader);
            decoder
                .read_to_end(&mut bytes)
                .with_context(|| format!("failed to read file {path:?}"))
        })?;
    Ok(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
#[inline]
pub(crate) fn filename(path: &Path) -> &str {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_else(|| {
            log::warn!("invalid rom_path: {path:?}");
            "??"
        })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
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
