//! Serialization/Deserialization for internal game state
//!
//! Converts primative types and arrays of primatives from/to Big-Endian byte arrays and writes
//! them to a file handle that implements Read/Write.

use crate::{nes_err, NesResult};
use std::{
    collections::VecDeque,
    io::{Read, Write},
};

const SAVE_FILE_MAGIC: [u8; 9] = *b"RUSTYNES\x1a";
// MAJOR version of SemVer. Increases when save file format isn't backwards compatible
const VERSION: u8 = 0;

/// Writes a header including a magic string and a version
pub fn write_save_header(fh: &mut dyn Write) -> NesResult<()> {
    SAVE_FILE_MAGIC.save(fh)?;
    VERSION.save(fh)
}

/// Validates a file to ensure it matches the current version and magic
pub fn validate_save_header(fh: &mut dyn Read) -> NesResult<()> {
    let mut magic = [0u8; 9];
    magic.load(fh)?;
    if magic != SAVE_FILE_MAGIC {
        nes_err!("invalid save file format")
    } else {
        let mut version = 0u8;
        version.load(fh)?;
        if version != VERSION {
            nes_err!(
                "invalid save file version. current: {}, save file: {}",
                VERSION,
                version,
            )
        } else {
            Ok(())
        }
    }
}

/// Savable trait
pub trait Savable {
    fn save(&self, _fh: &mut dyn Write) -> NesResult<()> {
        Ok(())
    }
    fn load(&mut self, _fh: &mut dyn Read) -> NesResult<()> {
        Ok(())
    }
}

impl Savable for bool {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        fh.write_all(&[*self as u8])?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut bytes = [0; 1];
        fh.read_exact(&mut bytes)?;
        *self = bytes[0] > 0;
        Ok(())
    }
}

impl Savable for char {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.to_string().save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut s = " ".to_string();
        s.load(fh)?;
        Ok(())
    }
}

impl Savable for i8 {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut bytes = [0; 1];
        fh.read_exact(&mut bytes)?;
        *self = i8::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for u8 {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut bytes = [0; 1];
        fh.read_exact(&mut bytes)?;
        *self = u8::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for i16 {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut bytes = [0; 2];
        fh.read_exact(&mut bytes)?;
        *self = i16::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for u16 {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut bytes = [0; 2];
        fh.read_exact(&mut bytes)?;
        *self = u16::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for i32 {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut bytes = [0u8; 4];
        fh.read_exact(&mut bytes)?;
        *self = i32::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for u32 {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut bytes = [0u8; 4];
        fh.read_exact(&mut bytes)?;
        *self = u32::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for f32 {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.to_bits().save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u32;
        val.load(fh)?;
        *self = f32::from_bits(val);
        Ok(())
    }
}

impl Savable for u64 {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut bytes = [0u8; 8];
        fh.read_exact(&mut bytes)?;
        *self = u64::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for f64 {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.to_bits().save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u64;
        val.load(fh)?;
        *self = f64::from_bits(val);
        Ok(())
    }
}

impl Savable for usize {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        let val = *self as u32;
        fh.write_all(&val.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut bytes = [0u8; 4];
        fh.read_exact(&mut bytes)?;
        let val = u32::from_be_bytes(bytes);
        *self = val as usize;
        Ok(())
    }
}

impl<T: Savable> Savable for [T] {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        let len: usize = self.len();
        if len > std::u32::MAX as usize {
            return nes_err!("Unable to save more than {} bytes", std::u32::MAX);
        }
        let len = len as u32;
        len.save(fh)?;
        for i in self.iter() {
            i.save(fh)?;
        }
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut len = 0u32;
        len.load(fh)?;
        if len > self.len() as u32 {
            return nes_err!("Array read len does not match");
        }
        for i in 0..len {
            self[i as usize].load(fh)?;
        }
        Ok(())
    }
}

impl<T: Savable + Default> Savable for Vec<T> {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        let len: usize = self.len();
        if len > std::u32::MAX as usize {
            return nes_err!("Unable to save more than {} bytes", std::u32::MAX);
        }
        let len = len as u32;
        len.save(fh)?;
        for i in self.iter() {
            i.save(fh)?;
        }
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut len = 0u32;
        len.load(fh)?;
        if self.is_empty() {
            *self = Vec::with_capacity(len as usize);
            for _ in 0..len {
                self.push(T::default());
            }
        } else if len != self.len() as u32 {
            return nes_err!("Vec read len does not match");
        }
        for i in 0..len {
            self[i as usize].load(fh)?;
        }
        Ok(())
    }
}

impl<T: Savable + Default> Savable for VecDeque<T> {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        let len: usize = self.len();
        if len > std::u32::MAX as usize {
            return nes_err!("Unable to save more than {} bytes", std::u32::MAX);
        }
        let len = len as u32;
        len.save(fh)?;
        for i in self.iter() {
            i.save(fh)?;
        }
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut len = 0u32;
        len.load(fh)?;
        if self.is_empty() {
            *self = VecDeque::with_capacity(len as usize);
            for _ in 0..len {
                self.push_back(T::default());
            }
        } else if len != self.len() as u32 {
            return nes_err!("VecDeque read len does not match");
        }
        for i in 0..len {
            self[i as usize].load(fh)?;
        }
        Ok(())
    }
}

impl<T: Savable + Default> Savable for Option<T> {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        match self {
            Some(t) => {
                1u8.save(fh)?;
                t.save(fh)?;
            }
            None => 0u8.save(fh)?,
        }
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut some = 0u8;
        some.load(fh)?;
        *self = match some {
            0 => None,
            1 => {
                let mut val = T::default();
                val.load(fh)?;
                Some(val)
            }
            _ => return nes_err!("invalid Option<T> read"),
        };
        Ok(())
    }
}

impl Savable for String {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.as_bytes().save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut len = 0u32;
        len.load(fh)?;
        if self.is_empty() {
            let mut bytes = Vec::with_capacity(len as usize);
            bytes.load(fh)?;
            *self = String::from_utf8(bytes)?;
        } else if len != self.len() as u32 {
            return nes_err!("String read len does not match");
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
