//! Serialization/Deserialization for internal game state
//!
//! Converts primative types and arrays of primatives from/to Big-Endian byte arrays and writes
//! them to a file handle that implements Read/Write.

use crate::{mapper::MapperType, NesResult};
use anyhow::anyhow;
use enum_dispatch::enum_dispatch;
use std::{
    collections::VecDeque,
    io::{Read, Write},
};

const SAVE_FILE_MAGIC_LEN: usize = 8;
const SAVE_FILE_MAGIC: [u8; SAVE_FILE_MAGIC_LEN] = *b"TETANES\x1a";
// MAJOR version of SemVer. Increases when save file format isn't backwards compatible
const VERSION: u8 = 0;

/// Writes a header including a magic string and a version
///
/// # Errors
///
/// If the header fails to write to disk, then an error is returned.
pub fn write_save_header<F: Write>(fh: &mut F) -> NesResult<()> {
    SAVE_FILE_MAGIC.save(fh)?;
    VERSION.save(fh)
}

/// Verifies a `TetaNES` saved state header.
///
/// # Errors
///
/// If the header fails to validate, then an error is returned.
pub fn validate_save_header<F: Read>(fh: &mut F) -> NesResult<()> {
    let mut magic = [0u8; SAVE_FILE_MAGIC_LEN];
    magic.load(fh)?;
    if magic == SAVE_FILE_MAGIC {
        let mut version = 0u8;
        version.load(fh)?;
        if version == VERSION {
            Ok(())
        } else {
            Err(anyhow!(
                "invalid save file version. current: {}, save file: {}",
                VERSION,
                version,
            ))
        }
    } else {
        Err(anyhow!("invalid save file format"))
    }
}

/// Savable trait
#[enum_dispatch(MapperType)]
pub trait Savable {
    /// Saves a given type to the file handle.
    ///
    /// # Errors
    ///
    /// If data fails to serialize, then an error is returned.
    fn save<F: Write>(&self, _fh: &mut F) -> NesResult<()> {
        Ok(())
    }

    /// Loads a given type from the file handle.
    ///
    /// # Errors
    ///
    /// If data fails to deserialize, then an error is returned.
    fn load<F: Read>(&mut self, _fh: &mut F) -> NesResult<()> {
        Ok(())
    }
}

impl Savable for bool {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        fh.write_all(&[*self as u8])?;
        Ok(())
    }

    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut bytes = [0; 1];
        fh.read_exact(&mut bytes)?;
        *self = bytes[0] > 0;
        Ok(())
    }
}

impl Savable for char {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.to_string().save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut s = " ".to_string();
        s.load(fh)?;
        Ok(())
    }
}

impl Savable for i8 {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut bytes = [0; 1];
        fh.read_exact(&mut bytes)?;
        *self = i8::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for u8 {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut bytes = [0; 1];
        fh.read_exact(&mut bytes)?;
        *self = u8::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for i16 {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut bytes = [0; 2];
        fh.read_exact(&mut bytes)?;
        *self = i16::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for u16 {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut bytes = [0; 2];
        fh.read_exact(&mut bytes)?;
        *self = u16::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for i32 {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut bytes = [0u8; 4];
        fh.read_exact(&mut bytes)?;
        *self = i32::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for u32 {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut bytes = [0u8; 4];
        fh.read_exact(&mut bytes)?;
        *self = u32::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for f32 {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.to_bits().save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u32;
        val.load(fh)?;
        *self = f32::from_bits(val);
        Ok(())
    }
}

impl Savable for u64 {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut bytes = [0u8; 8];
        fh.read_exact(&mut bytes)?;
        *self = u64::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for f64 {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.to_bits().save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u64;
        val.load(fh)?;
        *self = f64::from_bits(val);
        Ok(())
    }
}

impl Savable for usize {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        let val = *self as u32;
        fh.write_all(&val.to_be_bytes())?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut bytes = [0u8; 4];
        fh.read_exact(&mut bytes)?;
        let val = u32::from_be_bytes(bytes);
        *self = val as usize;
        Ok(())
    }
}

impl<T: Savable> Savable for [T] {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        let len: usize = self.len();
        if len > u32::MAX as usize {
            return Err(anyhow!("unable to save more than {} bytes", u32::MAX));
        }
        let len = len as u32;
        len.save(fh)?;
        for i in self.iter() {
            i.save(fh)?;
        }
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut len = 0u32;
        len.load(fh)?;
        assert!(
            len == self.len() as u32,
            "Array read len does not match. Got {}, expected {}",
            len,
            self.len() as u32
        );
        for i in 0..len {
            self[i as usize].load(fh)?;
        }
        Ok(())
    }
}

impl<T: Savable + Default> Savable for Vec<T> {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        let len: usize = self.len();
        if len > u32::MAX as usize {
            return Err(anyhow!("unable to save more than {} bytes", u32::MAX));
        }
        let len = len as u32;
        len.save(fh)?;
        for i in self.iter() {
            i.save(fh)?;
        }
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut len = 0u32;
        len.load(fh)?;
        if self.is_empty() {
            *self = Vec::with_capacity(len as usize);
            for _ in 0..len {
                self.push(T::default());
            }
        } else if len != self.len() as u32 {
            return Err(anyhow!(
                "Vec read len does not match. Got {}, expected {}",
                len,
                self.len() as u32
            ));
        }
        for i in 0..len {
            self[i as usize].load(fh)?;
        }
        Ok(())
    }
}

impl<T: Savable + Default> Savable for VecDeque<T> {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        let len: usize = self.len();
        if len > u32::MAX as usize {
            return Err(anyhow!("unable to save more than {} bytes", u32::MAX));
        }
        let len = len as u32;
        len.save(fh)?;
        for i in self.iter() {
            i.save(fh)?;
        }
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut len = 0u32;
        len.load(fh)?;
        if self.is_empty() {
            *self = VecDeque::with_capacity(len as usize);
            for _ in 0..len {
                self.push_back(T::default());
            }
        } else if len != self.len() as u32 {
            return Err(anyhow!(
                "VecDeque read len does not match. Got {}, expected {}",
                len,
                self.len() as u32
            ));
        }
        for i in 0..len {
            self[i as usize].load(fh)?;
        }
        Ok(())
    }
}

impl<T: Savable + Default> Savable for Option<T> {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        match self {
            Some(t) => {
                1u8.save(fh)?;
                t.save(fh)?;
            }
            None => 0u8.save(fh)?,
        }
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut some = 0u8;
        some.load(fh)?;
        *self = match some {
            0 => None,
            1 => {
                let mut val = T::default();
                val.load(fh)?;
                Some(val)
            }
            _ => return Err(anyhow!("invalid Option<T> read")),
        };
        Ok(())
    }
}

impl Savable for String {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.as_bytes().save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut len = 0u32;
        len.load(fh)?;
        if self.is_empty() {
            let mut bytes = Vec::with_capacity(len as usize);
            bytes.load(fh)?;
            *self = String::from_utf8(bytes)?;
        } else if len != self.len() as u32 {
            return Err(anyhow!(
                "string read len does not match. got {}, expected {}",
                len,
                self.len() as u32
            ));
        }
        Ok(())
    }
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
