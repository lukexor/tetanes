//! Memory types for dealing with bytes of u8

use crate::{serialization::Savable, NesResult};
use rand::Rng;
use std::{
    fmt,
    io::{Read, Write},
    ops::{Deref, DerefMut},
};

pub static mut RANDOMIZE_RAM: bool = false;

/// Memory Trait
pub trait Memory {
    fn read(&mut self, _addr: u16) -> u8 {
        0
    }
    fn peek(&self, _addr: u16) -> u8 {
        0
    }
    fn write(&mut self, _addr: u16, _val: u8) {}
}

impl fmt::Debug for dyn Memory {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "")
    }
}

#[derive(Clone)]
pub struct Ram(Vec<u8>);

impl Ram {
    pub fn init(size: usize) -> Self {
        let randomize = unsafe { RANDOMIZE_RAM };
        let ram = if randomize {
            let mut rng = rand::thread_rng();
            let mut ram = Vec::with_capacity(size);
            for _ in 0..size {
                ram.push(rng.gen_range(0x00, 0xFF));
            }
            ram
        } else {
            vec![0u8; size]
        };
        Self(ram)
    }
    pub fn null() -> Self {
        Self(Vec::new())
    }
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
    pub fn from_vec(v: Vec<u8>) -> Self {
        Self(v)
    }
    pub fn clear(&mut self) {
        self.0.clear()
    }
}

impl Memory for Ram {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }
    fn peek(&self, addr: u16) -> u8 {
        if self.0.is_empty() {
            return 0;
        }
        let addr = addr as usize % self.0.len();
        self.0[addr]
    }
    fn write(&mut self, addr: u16, val: u8) {
        if self.0.is_empty() {
            return;
        }
        let addr = addr as usize % self.0.len();
        self.0[addr] = val;
    }
}

impl Savable for Ram {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.0.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.0.load(fh)
    }
}

impl Bankable for Ram {
    fn chunks(&self, size: usize) -> Vec<Ram> {
        let mut chunks: Vec<Ram> = Vec::new();
        for slice in self.0.chunks(size) {
            chunks.push(Ram::from_bytes(slice));
        }
        chunks
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for Ram {
    fn default() -> Self {
        Self::init(0)
    }
}

impl fmt::Debug for Ram {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Ram {{ len: {} KB }}", self.0.len() / 1024)
    }
}

impl Deref for Ram {
    type Target = Vec<u8>;
    fn deref(&self) -> &Vec<u8> {
        &self.0
    }
}

impl DerefMut for Ram {
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }
}

#[derive(Clone)]
pub struct Rom(Vec<u8>);

impl Rom {
    pub fn init(size: usize) -> Self {
        Self(vec![0u8; size as usize])
    }
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
    pub fn from_vec(v: Vec<u8>) -> Self {
        Self(v)
    }
    pub fn to_ram(&self) -> Ram {
        Ram::from_vec(self.0.clone())
    }
}

impl Memory for Rom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }
    fn peek(&self, addr: u16) -> u8 {
        if self.0.is_empty() {
            return 0;
        }
        let addr = addr as usize % self.0.len();
        self.0[addr]
    }
    fn write(&mut self, _addr: u16, _val: u8) {} // ROM is read-only
}

impl Savable for Rom {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.0.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.0.load(fh)
    }
}

impl Bankable for Rom {
    fn chunks(&self, size: usize) -> Vec<Rom> {
        let mut chunks: Vec<Rom> = Vec::new();
        for slice in self.0.chunks(size) {
            chunks.push(Rom::from_bytes(slice));
        }
        chunks
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for Rom {
    fn default() -> Self {
        Self::init(0)
    }
}

impl fmt::Debug for Rom {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Rom {{ len: {} KB }}", self.0.len() / 1024)
    }
}

impl Deref for Rom {
    type Target = Vec<u8>;
    fn deref(&self) -> &Vec<u8> {
        &self.0
    }
}

pub trait Bankable
where
    Self: std::marker::Sized,
{
    fn chunks(&self, size: usize) -> Vec<Self>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

#[derive(Clone)]
pub struct Banks<T>
where
    T: Memory + Bankable,
{
    banks: Vec<T>,
    pub size: usize,
}

impl<T> Banks<T>
where
    T: Memory + Bankable,
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

impl<T> fmt::Debug for Banks<T>
where
    T: Memory + Bankable,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(
            f,
            "Rom {{ len: {}, size: {} KB  }}",
            self.banks.len(),
            self.size / 1024,
        )
    }
}

impl<T> Deref for Banks<T>
where
    T: Memory + Bankable,
{
    type Target = Vec<T>;
    fn deref(&self) -> &Vec<T> {
        &self.banks
    }
}

impl<T> DerefMut for Banks<T>
where
    T: Memory + Bankable,
{
    fn deref_mut(&mut self) -> &mut Vec<T> {
        &mut self.banks
    }
}

impl<T> Default for Banks<T>
where
    T: Memory + Bankable,
{
    fn default() -> Self {
        Self::new()
    }
}
