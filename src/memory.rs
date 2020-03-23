//! Memory types for dealing with bytes of u8

use crate::{serialization::Savable, NesResult};
use rand::Rng;
use std::{
    fmt,
    io::{Read, Write},
    ops::{Deref, DerefMut},
};

pub trait MemRead {
    fn read(&mut self, _addr: u16) -> u8 {
        0
    }
    fn readw(&mut self, _addr: usize) -> u8 {
        0
    }
    fn peek(&self, _addr: u16) -> u8 {
        0
    }
    fn peekw(&self, _addr: usize) -> u8 {
        0
    }
}
pub trait MemWrite {
    fn write(&mut self, _addr: u16, _val: u8) {}
    fn writew(&mut self, _addr: usize, _val: u8) {}
}
pub trait Bankable
where
    Self: std::marker::Sized,
{
    fn chunks(&self, size: usize) -> Vec<Self>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

#[derive(Default, Clone)]
pub struct Memory {
    data: Vec<u8>,
    writable: bool,
}

#[derive(Default, Clone)]
pub struct Banks<T>
where
    T: MemRead + MemWrite + Bankable,
{
    banks: Vec<T>,
    pub size: usize,
}

impl Memory {
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let randomize = cfg!(not(feature = "no-randomize-ram"));
        let data = if randomize {
            let mut rng = rand::thread_rng();
            let mut data = Vec::with_capacity(capacity);
            for _ in 0..capacity {
                data.push(rng.gen_range(0x00, 0xFF));
            }
            data
        } else {
            vec![0u8; capacity]
        };
        Self {
            data,
            writable: true,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut memory = Self::with_capacity(bytes.len());
        memory.data = bytes.to_vec();
        memory
    }

    pub fn rom(capacity: usize) -> Self {
        let mut rom = Self::with_capacity(capacity);
        rom.writable = false;
        rom
    }
    pub fn rom_from_bytes(bytes: &[u8]) -> Self {
        let mut rom = Self::rom(bytes.len());
        rom.data = bytes.to_vec();
        rom
    }

    pub fn ram(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }
    pub fn ram_from_bytes(bytes: &[u8]) -> Self {
        let mut ram = Self::ram(bytes.len());
        ram.data = bytes.to_vec();
        ram
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl MemRead for Memory {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }
    fn readw(&mut self, addr: usize) -> u8 {
        self.peekw(addr)
    }
    fn peek(&self, addr: u16) -> u8 {
        self.peekw(addr as usize)
    }
    fn peekw(&self, addr: usize) -> u8 {
        if !self.data.is_empty() {
            let addr = addr % self.data.len();
            self.data[addr]
        } else {
            0
        }
    }
}

impl MemWrite for Memory {
    fn write(&mut self, addr: u16, val: u8) {
        self.writew(addr as usize, val);
    }
    fn writew(&mut self, addr: usize, val: u8) {
        if self.writable && !self.data.is_empty() {
            let addr = addr % self.data.len();
            self.data[addr] = val;
        }
    }
}

impl Bankable for Memory {
    fn chunks(&self, size: usize) -> Vec<Memory> {
        let mut chunks: Vec<Memory> = Vec::new();
        for slice in self.data.chunks(size) {
            let mut chunk = Memory::from_bytes(slice);
            chunk.writable = self.writable;
            chunks.push(chunk);
        }
        chunks
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl Savable for Memory {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.data.save(fh)?;
        self.writable.save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.data.load(fh)?;
        self.writable.load(fh)?;
        Ok(())
    }
}

impl<T> Banks<T>
where
    T: MemRead + MemWrite + Bankable,
{
    pub fn new() -> Self {
        Self {
            banks: vec![],
            size: 0usize,
        }
    }

    pub fn init(data: &T, size: usize) -> Self {
        let mut banks: Vec<T> = Vec::with_capacity(data.len());
        if data.len() > 0 {
            for bank in data.chunks(size) {
                banks.push(bank);
            }
        }
        Self { banks, size }
    }
}

impl<T> Deref for Banks<T>
where
    T: MemRead + MemWrite + Bankable,
{
    type Target = Vec<T>;
    fn deref(&self) -> &Vec<T> {
        &self.banks
    }
}

impl<T> DerefMut for Banks<T>
where
    T: MemRead + MemWrite + Bankable,
{
    fn deref_mut(&mut self) -> &mut Vec<T> {
        &mut self.banks
    }
}

impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(
            f,
            "Memory {{ data: {} KB, writable: {} }}",
            self.data.len() / 1024,
            self.writable
        )
    }
}

impl<T> fmt::Debug for Banks<T>
where
    T: MemRead + MemWrite + Bankable,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(
            f,
            "Bank {{ len: {}, size: {} KB  }}",
            self.banks.len(),
            self.size / 1024,
        )
    }
}
