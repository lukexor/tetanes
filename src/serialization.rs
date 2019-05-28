//! Serialization/Deserialization for internal game state
//!
//! Converts primative types and arrays of primatives from/to Big-Endian byte arrays and writes
//! them to a file handle that implements Read/Write.

use crate::util::Result;
use std::io::Read;
use std::io::Write;

/// Savable trait
pub trait Savable {
    fn save(&self, fh: &mut Write) -> Result<()>;
    fn load(&mut self, fh: &mut Read) -> Result<()>;
}

impl Savable for bool {
    fn save(&self, fh: &mut Write) -> Result<()> {
        fh.write_all(&[*self as u8])?;
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        let mut bytes = [0; 1];
        fh.read_exact(&mut bytes)?;
        *self = bytes[0] > 0;
        Ok(())
    }
}

impl Savable for u8 {
    fn save(&self, fh: &mut Write) -> Result<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        let mut bytes = [0; 1];
        fh.read_exact(&mut bytes)?;
        *self = bytes[0];
        Ok(())
    }
}

impl Savable for u16 {
    fn save(&self, fh: &mut Write) -> Result<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        let mut bytes = [0; 2];
        fh.read_exact(&mut bytes)?;
        *self = u16::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for i32 {
    fn save(&self, fh: &mut Write) -> Result<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        let mut bytes = [0u8; 4];
        fh.read_exact(&mut bytes)?;
        *self = i32::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for u32 {
    fn save(&self, fh: &mut Write) -> Result<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        let mut bytes = [0u8; 4];
        fh.read_exact(&mut bytes)?;
        *self = u32::from_be_bytes(bytes);
        Ok(())
    }
}

impl Savable for u64 {
    fn save(&self, fh: &mut Write) -> Result<()> {
        fh.write_all(&self.to_be_bytes())?;
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        let mut bytes = [0u8; 8];
        fh.read_exact(&mut bytes)?;
        *self = u64::from_be_bytes(bytes);
        Ok(())
    }
}

impl<T: Savable> Savable for [T] {
    fn save(&self, fh: &mut Write) -> Result<()> {
        let len: usize = self.len();
        if len > std::u32::MAX as usize {
            panic!("Unable to save more than {} bytes", std::u32::MAX);
        }
        let len = len as u32;
        len.save(fh)?;
        for i in self.iter() {
            i.save(fh)?;
        }
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        let mut len = 0u32;
        len.load(fh)?;
        for i in 0..len {
            self[i as usize].load(fh)?;
        }
        Ok(())
    }
}

impl<T: Savable + Default> Savable for Vec<T> {
    fn save(&self, fh: &mut Write) -> Result<()> {
        let len: usize = self.len();
        if len > std::u32::MAX as usize {
            panic!("Unable to save more than {} bytes", std::u32::MAX);
        }
        let len = len as u32;
        len.save(fh)?;
        for i in self.iter() {
            i.save(fh)?;
        }
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        let mut len = 0u32;
        len.load(fh)?;
        self.truncate(0);
        self.reserve(len as usize);
        for _ in 0..len {
            let mut x: T = Default::default();
            x.load(fh)?;
            self.push(x);
        }
        Ok(())
    }
}
