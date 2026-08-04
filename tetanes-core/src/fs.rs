//! Filesystem utilities for save state and compression.

use crate::sys::fs;
use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;
use tracing::warn;

const SAVE_FILE_MAGIC_LEN: usize = 8;
const SAVE_FILE_MAGIC: [u8; SAVE_FILE_MAGIC_LEN] = *b"TETANES\x1a";
// Keep this separate from Semver because breaking API changes may not invalidate the save format.
const SAVE_VERSION: &str = "2";
/// Version for the bundled game database.
///
/// Deliberately independent of `SAVE_VERSION`: the database is not save-state data, so bumping
/// the save format must not invalidate it.
pub const GAME_DB_VERSION: &str = "1";
/// Version for battery-backed cart RAM.
///
/// Deliberately independent of `SAVE_VERSION` for the same reason as [`GAME_DB_VERSION`]: a
/// `.sram` file holds a board's battery contents, not a snapshot of console state, so a save-state
/// layout change must not make a player's save games unreadable. Bump this only when a board
/// changes what it writes.
pub const SRAM_VERSION: &str = "1";

/// A `Result` from a save-file operation.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors from reading or writing a save file, save state or SRAM.
#[derive(Error, Debug)]
#[must_use]
pub enum Error {
    /// The file's magic or version did not match what this build writes.
    #[error("invalid tetanes header: {0}")]
    InvalidHeader(String),
    /// The header could not be written.
    #[error("failed to write tetanes header: {0:?}")]
    WriteHeaderFailed(std::io::Error),
    /// Deflate compression failed.
    #[error("failed to encode data: {0:?}")]
    EncodingFailed(std::io::Error),
    /// Deflate decompression failed.
    #[error("failed to decode data: {0:?}")]
    DecodingFailed(std::io::Error),
    /// The value could not be serialized.
    #[error("failed to serialize data: {0:?}")]
    SerializationFailed(String),
    /// The bytes could not be deserialized into the expected type.
    #[error("failed to deserialize data: {0:?}")]
    DeserializationFailed(String),
    /// The path was not usable, e.g. it had no filename.
    #[error("invalid path: {0:?}")]
    InvalidPath(PathBuf),
    /// An underlying I/O error, with what was being done at the time.
    #[error("{context}: {source:?}")]
    Io {
        /// The underlying I/O error.
        source: std::io::Error,
        /// What was being read or written.
        context: String,
    },
    /// A platform backend's own error.
    #[error("{0}")]
    Custom(String),
}

impl Error {
    /// Wraps an I/O error with a description of what was being done.
    pub fn io(source: std::io::Error, context: impl Into<String>) -> Self {
        Self::Io {
            source,
            context: context.into(),
        }
    }

    /// Builds an error from a platform backend's own message.
    pub fn custom(error: impl Into<String>) -> Self {
        Self::Custom(error.into())
    }
}

/// Writes a header including a magic string and a version
///
/// # Errors
///
/// If the header fails to write to disk, then an error is returned.
pub fn write_header(f: &mut impl Write, version: &str) -> std::io::Result<()> {
    f.write_all(&SAVE_FILE_MAGIC)?;
    f.write_all(version.as_bytes())
}

/// Verifies a `TetaNES` saved state header.
///
/// # Errors
///
/// If the header fails to validate, then an error is returned.
pub fn validate_header(f: &mut impl Read, expected: &str) -> Result<()> {
    let mut magic = [0u8; SAVE_FILE_MAGIC_LEN];
    f.read_exact(&mut magic)
        .map_err(|s| Error::InvalidHeader(s.to_string()))?;
    if magic != SAVE_FILE_MAGIC {
        // Both are printed as text: the reader of this message is comparing it against a file
        // format, not a byte array.
        let expected = String::from_utf8_lossy(&SAVE_FILE_MAGIC);
        let found = String::from_utf8_lossy(&magic);
        return Err(Error::InvalidHeader(format!(
            "invalid magic (expected {expected:?}, found {found:?})",
        )));
    }

    let mut version = [0u8];
    f.read_exact(&mut version)
        .map_err(|s| Error::InvalidHeader(s.to_string()))?;
    if version == expected.as_bytes() {
        Ok(())
    } else {
        let found = String::from_utf8_lossy(&version);
        Err(Error::InvalidHeader(format!(
            "invalid version (expected {expected:?}, found {found:?})",
        )))
    }
}

/// Deflate-compresses `data` into `writer`.
///
/// # Errors
///
/// If the writer fails.
pub fn encode(mut writer: &mut impl Write, data: &[u8]) -> std::io::Result<()> {
    let mut encoder = DeflateEncoder::new(&mut writer, Compression::default());
    encoder.write_all(data)?;
    encoder.finish()?;
    Ok(())
}

/// Deflate-decompresses `data`.
///
/// # Errors
///
/// If the reader fails or the stream is not valid deflate.
pub fn decode(data: impl Read) -> std::io::Result<Vec<u8>> {
    let mut decoded = vec![];
    let mut decoder = DeflateDecoder::new(data);
    decoder.read_to_end(&mut decoded)?;
    Ok(decoded)
}

/// Serializes, compresses and writes `value` to `writer`, behind the current save header.
///
/// Use [`save_path`] to write to the filesystem.
///
/// # Errors
///
/// If the value cannot be serialized or the writer fails.
pub fn save<T>(writer: impl Write, value: &T) -> Result<()>
where
    T: ?Sized + Serialize,
{
    save_version(writer, value, SAVE_VERSION)
}

/// Save data with an explicit format version.
///
/// # Errors
///
/// If the data fails to serialize or write, then an error is returned.
pub fn save_version<T>(mut writer: impl Write, value: &T, version: &str) -> Result<()>
where
    T: ?Sized + Serialize,
{
    let config = bincode::config::legacy();
    let data = bincode::serde::encode_to_vec(value, config)
        .map_err(|err| Error::SerializationFailed(err.to_string()))?;
    write_header(&mut writer, version).map_err(Error::WriteHeaderFailed)?;
    encode(&mut writer, &data).map_err(Error::EncodingFailed)?;
    Ok(())
}

/// Serializes, compresses and writes a board's battery-backed RAM to `writer`.
///
/// # Errors
///
/// If the value cannot be serialized or the writer fails.
pub fn save_sram<T>(writer: impl Write, value: &T) -> Result<()>
where
    T: ?Sized + Serialize,
{
    save_version(writer, value, SRAM_VERSION)
}

/// Reads a board's battery-backed RAM from `reader`.
///
/// Use [`load_sram_path`] to read from the filesystem, which additionally preserves a save it
/// cannot parse.
///
/// # Errors
///
/// If the reader fails, its header does not match, or it does not hold a `T`.
pub fn load_sram<T>(reader: impl Read) -> Result<T>
where
    T: DeserializeOwned,
{
    load_version(reader, SRAM_VERSION)
}

/// Copies an unreadable save alongside itself as `<name>.bak`.
///
/// Never overwrites an existing backup: the second run after a save broke would otherwise back up
/// the replacement and destroy the copy made by the first.
///
/// `err` is only reported, so a caller whose error type is not [`enum@Error`] can still call this.
pub fn back_up_sram(path: &Path, err: &dyn std::fmt::Display) {
    if !exists(path) {
        return;
    }
    let Some(name) = path.file_name() else {
        return;
    };
    let mut name = name.to_os_string();
    name.push(".bak");
    let backup = path.with_file_name(name);
    if exists(&backup) {
        warn!("failed to load {path:?} ({err}); {backup:?} already exists, leaving it alone");
        return;
    }
    match load_raw_path(path).and_then(|data| save_raw_path(&backup, &data)) {
        Ok(()) => warn!("failed to load {path:?} ({err}); backed it up to {backup:?}"),
        Err(backup_err) => {
            warn!("failed to load {path:?} ({err}); backing it up also failed: {backup_err}");
        }
    }
}

/// Writes bytes to `writer` with no header, compression or serialization.
///
/// # Errors
///
/// If the writer fails.
pub fn save_raw(mut writer: impl Write, value: &[u8]) -> Result<()> {
    writer
        .write_all(value)
        .map_err(|err| Error::io(err, "failed to save data"))
}

/// Reads, decompresses and deserializes a value from `reader`.
///
/// Use [`load_path`] to read from the filesystem.
///
/// # Errors
///
/// If the reader fails, its header does not match, or it does not hold a `T`.
pub fn load<T>(reader: impl Read) -> Result<T>
where
    T: DeserializeOwned,
{
    load_version(reader, SAVE_VERSION)
}

/// Load data written with an explicit format version.
///
/// # Errors
///
/// If the reader fails, its header does not match, or it does not hold a `T`.
pub fn load_version<T>(mut reader: impl Read, version: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    validate_header(&mut reader, version)?;
    let data = decode(&mut reader).map_err(Error::DecodingFailed)?;
    let config = bincode::config::legacy();
    let (res, _) = bincode::serde::decode_from_slice(&data, config)
        .map_err(|err| Error::DeserializationFailed(err.to_string()))?;
    Ok(res)
}

/// Reads bytes from `reader` with no header, decompression or deserialization.
///
/// # Errors
///
/// If the reader fails.
pub fn load_raw(mut reader: impl Read) -> Result<Vec<u8>> {
    let mut data = vec![];
    reader
        .read_to_end(&mut data)
        .map_err(|err| Error::io(err, "failed to load data"))?;
    Ok(data)
}

/// Serializes, compresses and writes `value` to `path`, behind the current save header.
///
/// # Errors
///
/// If the value cannot be serialized or the file cannot be written.
pub fn save_path<T>(path: impl AsRef<Path>, value: &T) -> Result<()>
where
    T: ?Sized + Serialize,
{
    save_version_path(path, value, SAVE_VERSION)
}

/// Writes data to `path` with an explicit format version.
///
/// # Errors
///
/// If the data fails to serialize or the file cannot be written.
pub fn save_version_path<T>(path: impl AsRef<Path>, value: &T, version: &str) -> Result<()>
where
    T: ?Sized + Serialize,
{
    let mut writer = fs::writer_impl(path)?;
    save_version(&mut writer, value, version)?;
    // The wasm backend buffers the whole file and commits to local storage on flush, so the write
    // has not happened until this returns.
    writer
        .flush()
        .map_err(|err| Error::io(err, "failed to save data"))
}

/// Serializes, compresses and writes a board's battery-backed RAM to `path`.
///
/// # Errors
///
/// If the value cannot be serialized or the file cannot be written.
pub fn save_sram_path<T>(path: impl AsRef<Path>, value: &T) -> Result<()>
where
    T: ?Sized + Serialize,
{
    save_version_path(path, value, SRAM_VERSION)
}

/// Writes bytes to `path` with no header, compression or serialization.
///
/// # Errors
///
/// If the file cannot be written.
pub fn save_raw_path(path: impl AsRef<Path>, value: &[u8]) -> Result<()> {
    let mut writer = fs::writer_impl(path)?;
    save_raw(&mut writer, value)?;
    writer
        .flush()
        .map_err(|err| Error::io(err, "failed to save data"))
}

/// Reads, decompresses and deserializes a value from `path`.
///
/// # Errors
///
/// If the file cannot be read, its header does not match, or it does not hold a `T`.
pub fn load_path<T>(path: impl AsRef<Path>) -> Result<T>
where
    T: DeserializeOwned,
{
    load_version_path(path, SAVE_VERSION)
}

/// Loads data from `path` written with an explicit format version.
///
/// # Errors
///
/// If the file cannot be read, its header does not match, or it does not hold a `T`.
pub fn load_version_path<T>(path: impl AsRef<Path>, version: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    load_version(fs::reader_impl(path)?, version)
}

/// Reads a board's battery-backed RAM from `path`, moving the file aside if it cannot be read.
///
/// A save that fails to load is not a save that may be discarded: the caller keeps playing and
/// writes this same path back out when the ROM is unloaded, so an unreadable file is overwritten
/// by whatever the console happens to hold. Copying it to `<name>.bak` first makes a bad header or
/// a truncated file cost a rename instead of the player's game.
///
/// # Errors
///
/// If the file cannot be read, its header does not match, or it does not hold a `T`.
pub fn load_sram_path<T>(path: impl AsRef<Path>) -> Result<T>
where
    T: DeserializeOwned,
{
    let path = path.as_ref();
    load_version_path(path, SRAM_VERSION).inspect_err(|err| back_up_sram(path, err))
}

/// Reads bytes from `path` with no header, decompression or deserialization.
///
/// # Errors
///
/// If the file cannot be read.
pub fn load_raw_path(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    load_raw(fs::reader_impl(path)?)
}

/// Removes a directory and everything in it.
///
/// # Errors
///
/// If the directory cannot be removed.
pub fn clear_dir(path: impl AsRef<Path>) -> Result<()> {
    fs::clear_dir_impl(path)
}

/// Whether `path` exists, on platforms that have a filesystem.
pub fn exists(path: &Path) -> bool {
    fs::exists_impl(path)
}

/// The final component of `path`, or an empty string.
pub fn filename(path: &Path) -> &str {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_else(|| {
            warn!("invalid path without file_name: {path:?}");
            "??"
        })
}

/// CRC32 of `data`, which is how ROMs are matched against the game database.
pub fn compute_crc32(data: &[u8]) -> u32 {
    compute_combine_crc32(0, data)
}

/// Extends an existing CRC32 with more data, for hashing PRG and CHR as one.
pub fn compute_combine_crc32(crc32: u32, data: &[u8]) -> u32 {
    const BUFFER_SIZE: usize = 0x2000;
    data.chunks(BUFFER_SIZE).fold(crc32, compute_crc32_buffer)
}

fn compute_crc32_buffer(crc32: u32, buffer: &[u8]) -> u32 {
    buffer.iter().fold(crc32 ^ 0xFFFFFFFF, |crc32, byte| {
        (crc32 >> 8) ^ CRC_TABLE[((crc32 ^ *byte as u32) & 0xFF) as usize]
    }) ^ 0xFFFFFFFF
}

const CRC_TABLE: [u32; 256] = [
    0x00000000, 0x77073096, 0xEE0E612C, 0x990951BA, 0x076DC419, 0x706AF48F, 0xE963A535, 0x9E6495A3,
    0x0EDB8832, 0x79DCB8A4, 0xE0D5E91E, 0x97D2D988, 0x09B64C2B, 0x7EB17CBD, 0xE7B82D07, 0x90BF1D91,
    0x1DB71064, 0x6AB020F2, 0xF3B97148, 0x84BE41DE, 0x1ADAD47D, 0x6DDDE4EB, 0xF4D4B551, 0x83D385C7,
    0x136C9856, 0x646BA8C0, 0xFD62F97A, 0x8A65C9EC, 0x14015C4F, 0x63066CD9, 0xFA0F3D63, 0x8D080DF5,
    0x3B6E20C8, 0x4C69105E, 0xD56041E4, 0xA2677172, 0x3C03E4D1, 0x4B04D447, 0xD20D85FD, 0xA50AB56B,
    0x35B5A8FA, 0x42B2986C, 0xDBBBC9D6, 0xACBCF940, 0x32D86CE3, 0x45DF5C75, 0xDCD60DCF, 0xABD13D59,
    0x26D930AC, 0x51DE003A, 0xC8D75180, 0xBFD06116, 0x21B4F4B5, 0x56B3C423, 0xCFBA9599, 0xB8BDA50F,
    0x2802B89E, 0x5F058808, 0xC60CD9B2, 0xB10BE924, 0x2F6F7C87, 0x58684C11, 0xC1611DAB, 0xB6662D3D,
    0x76DC4190, 0x01DB7106, 0x98D220BC, 0xEFD5102A, 0x71B18589, 0x06B6B51F, 0x9FBFE4A5, 0xE8B8D433,
    0x7807C9A2, 0x0F00F934, 0x9609A88E, 0xE10E9818, 0x7F6A0DBB, 0x086D3D2D, 0x91646C97, 0xE6635C01,
    0x6B6B51F4, 0x1C6C6162, 0x856530D8, 0xF262004E, 0x6C0695ED, 0x1B01A57B, 0x8208F4C1, 0xF50FC457,
    0x65B0D9C6, 0x12B7E950, 0x8BBEB8EA, 0xFCB9887C, 0x62DD1DDF, 0x15DA2D49, 0x8CD37CF3, 0xFBD44C65,
    0x4DB26158, 0x3AB551CE, 0xA3BC0074, 0xD4BB30E2, 0x4ADFA541, 0x3DD895D7, 0xA4D1C46D, 0xD3D6F4FB,
    0x4369E96A, 0x346ED9FC, 0xAD678846, 0xDA60B8D0, 0x44042D73, 0x33031DE5, 0xAA0A4C5F, 0xDD0D7CC9,
    0x5005713C, 0x270241AA, 0xBE0B1010, 0xC90C2086, 0x5768B525, 0x206F85B3, 0xB966D409, 0xCE61E49F,
    0x5EDEF90E, 0x29D9C998, 0xB0D09822, 0xC7D7A8B4, 0x59B33D17, 0x2EB40D81, 0xB7BD5C3B, 0xC0BA6CAD,
    0xEDB88320, 0x9ABFB3B6, 0x03B6E20C, 0x74B1D29A, 0xEAD54739, 0x9DD277AF, 0x04DB2615, 0x73DC1683,
    0xE3630B12, 0x94643B84, 0x0D6D6A3E, 0x7A6A5AA8, 0xE40ECF0B, 0x9309FF9D, 0x0A00AE27, 0x7D079EB1,
    0xF00F9344, 0x8708A3D2, 0x1E01F268, 0x6906C2FE, 0xF762575D, 0x806567CB, 0x196C3671, 0x6E6B06E7,
    0xFED41B76, 0x89D32BE0, 0x10DA7A5A, 0x67DD4ACC, 0xF9B9DF6F, 0x8EBEEFF9, 0x17B7BE43, 0x60B08ED5,
    0xD6D6A3E8, 0xA1D1937E, 0x38D8C2C4, 0x4FDFF252, 0xD1BB67F1, 0xA6BC5767, 0x3FB506DD, 0x48B2364B,
    0xD80D2BDA, 0xAF0A1B4C, 0x36034AF6, 0x41047A60, 0xDF60EFC3, 0xA867DF55, 0x316E8EEF, 0x4669BE79,
    0xCB61B38C, 0xBC66831A, 0x256FD2A0, 0x5268E236, 0xCC0C7795, 0xBB0B4703, 0x220216B9, 0x5505262F,
    0xC5BA3BBE, 0xB2BD0B28, 0x2BB45A92, 0x5CB36A04, 0xC2D7FFA7, 0xB5D0CF31, 0x2CD99E8B, 0x5BDEAE1D,
    0x9B64C2B0, 0xEC63F226, 0x756AA39C, 0x026D930A, 0x9C0906A9, 0xEB0E363F, 0x72076785, 0x05005713,
    0x95BF4A82, 0xE2B87A14, 0x7BB12BAE, 0x0CB61B38, 0x92D28E9B, 0xE5D5BE0D, 0x7CDCEFB7, 0x0BDBDF21,
    0x86D3D2D4, 0xF1D4E242, 0x68DDB3F8, 0x1FDA836E, 0x81BE16CD, 0xF6B9265B, 0x6FB077E1, 0x18B74777,
    0x88085AE6, 0xFF0F6A70, 0x66063BCA, 0x11010B5C, 0x8F659EFF, 0xF862AE69, 0x616BFFD3, 0x166CCF45,
    0xA00AE278, 0xD70DD2EE, 0x4E048354, 0x3903B3C2, 0xA7672661, 0xD06016F7, 0x4969474D, 0x3E6E77DB,
    0xAED16A4A, 0xD9D65ADC, 0x40DF0B66, 0x37D83BF0, 0xA9BCAE53, 0xDEBB9EC5, 0x47B2CF7F, 0x30B5FFE9,
    0xBDBDF21C, 0xCABAC28A, 0x53B39330, 0x24B4A3A6, 0xBAD03605, 0xCDD70693, 0x54DE5729, 0x23D967BF,
    0xB3667A2E, 0xC4614AB8, 0x5D681B02, 0x2A6F2B94, 0xB40BBE37, 0xC30C8EA1, 0x5A05DF1B, 0x2D02EF8D,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_header() {
        let mut file = Vec::new();
        assert!(
            write_header(&mut file, SAVE_VERSION).is_ok(),
            "write header"
        );
        assert!(
            validate_header(&mut file.as_slice(), SAVE_VERSION).is_ok(),
            "validate header"
        );
    }

    /// The caller keeps playing after a failed load and writes the same path back out on unload,
    /// so an unreadable save has to be copied aside at the moment it fails to read - and a second
    /// run must not then back up the replacement over the copy the first run made.
    #[test]
    fn an_unreadable_save_is_backed_up_once() {
        let path = std::env::temp_dir().join("tetanes-unreadable.sram");
        let backup = std::env::temp_dir().join("tetanes-unreadable.sram.bak");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);

        save_version_path(&path, &vec![1u8, 2, 3], "9").expect("writes a future version");
        let original = std::fs::read(&path).expect("reads back");

        let _ = load_sram_path::<Vec<u8>>(&path).expect_err("cannot be read");
        assert_eq!(std::fs::read(&backup).expect("backed up"), original);

        // Now the save has been replaced, as `unload_rom` would.
        save_sram_path(&path, &vec![9u8]).expect("saves");
        load_sram_path::<Vec<u8>>(&path).expect("readable now");
        assert_eq!(
            std::fs::read(&backup).expect("still there"),
            original,
            "the first backup survives"
        );

        std::fs::remove_file(&path).expect("cleans up");
        std::fs::remove_file(&backup).expect("cleans up");
    }

    /// The version is one ASCII byte, and a mismatch is the one message a player sees when a save
    /// stops loading - it has to name the version, not its byte value.
    #[test]
    fn a_version_mismatch_names_both_versions() {
        let mut file = Vec::new();
        write_header(&mut file, "2").expect("write header");

        let err = validate_header(&mut file.as_slice(), "1").expect_err("mismatched");
        assert_eq!(
            err.to_string(),
            "invalid tetanes header: invalid version (expected \"1\", found \"2\")"
        );
    }

    /// `.sram` files written by every released build carry version "1". Bumping `SAVE_VERSION`
    /// must not move this one, or every player's battery saves stop validating and the next
    /// `unload_rom` overwrites them with blank RAM.
    #[test]
    fn sram_header_is_pinned_to_the_released_version() {
        assert_eq!(SRAM_VERSION, "1");

        let mut file = Vec::new();
        write_header(&mut file, SRAM_VERSION).expect("write header");
        assert_eq!(&file[SAVE_FILE_MAGIC_LEN..], b"1");
    }

    #[test]
    fn crc32() {
        let s = "Lorem ipsum dolor sit amet, consectetur adipisicing elit";
        assert_eq!(compute_crc32(s.as_bytes()), 0xb9b4cbd5);
    }
}
