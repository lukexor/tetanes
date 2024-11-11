//! Memory and Bankswitching implementations.

use crate::common::{Reset, ResetKind};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    num::NonZeroUsize,
    ops::{Deref, DerefMut},
    str::FromStr,
};

/// Represents ROM or RAM memory in bytes, with a custom Debug implementation that avoids printing
/// the entire contents..
#[derive(Default, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Memory(Vec<u8>);

impl Memory {
    /// Create a new, empty `Memory` instance.
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Create a new `Memory` instance of a given size, zeroed out.
    pub fn with_size(size: usize) -> Self {
        Self(vec![0; size])
    }

    /// Create a new `Memory` instance of a given size based on [`RamState`], zeroed out.
    pub fn ram(state: RamState, size: usize) -> Self {
        let mut ram = Self::with_size(size);
        ram.fill_ram(state);
        ram
    }

    /// Fills `Memory` based on [`RamState`].
    pub fn fill_ram(&mut self, state: RamState) {
        match state {
            RamState::AllZeros => self.0.fill(0x00),
            RamState::AllOnes => self.0.fill(0xFF),
            RamState::Random => {
                let mut rng = rand::thread_rng();
                for val in &mut self.0 {
                    *val = rng.gen_range(0x00..=0xFF);
                }
            }
        }
    }
}

impl Reset for Memory {
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {}
    }
}

impl From<Vec<u8>> for Memory {
    fn from(val: Vec<u8>) -> Self {
        Self(val)
    }
}

impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Memory")
            .field("len", &self.0.len())
            .field("capacity", &self.0.capacity())
            .finish()
    }
}

impl Deref for Memory {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Memory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for Memory {
    type Item = u8;
    type IntoIter = std::vec::IntoIter<u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Memory {
    type Item = &'a u8;
    type IntoIter = std::slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut Memory {
    type Item = &'a mut u8;
    type IntoIter = std::slice::IterMut<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

/// A trait that represents [`Memory`] operations.
pub trait Mem {
    /// Read from the given address.
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    /// Peek from the given address.
    fn peek(&self, addr: u16) -> u8;

    /// Read two bytes from the given address.
    fn read_u16(&mut self, addr: u16) -> u16 {
        let lo = self.read(addr);
        let hi = self.read(addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    /// Peek two bytes from the given address.
    fn peek_u16(&self, addr: u16) -> u16 {
        let lo = self.peek(addr);
        let hi = self.peek(addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    /// Write value to the given address.
    fn write(&mut self, addr: u16, val: u8);

    /// Write  valuetwo bytes to the given address.
    fn write_u16(&mut self, addr: u16, val: u16) {
        let [lo, hi] = val.to_le_bytes();
        self.write(addr, lo);
        self.write(addr, hi);
    }
}

/// RAM [`Memory`] in a given state on startup.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum RamState {
    #[default]
    AllZeros,
    AllOnes,
    Random,
}

impl RamState {
    /// Return `RamState` options as a slice.
    pub const fn as_slice() -> &'static [Self] {
        &[Self::AllZeros, Self::AllOnes, Self::Random]
    }

    /// Return `RamState` as a `str`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AllZeros => "all-zeros",
            Self::AllOnes => "all-ones",
            Self::Random => "random",
        }
    }
}

impl From<usize> for RamState {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::AllZeros,
            1 => Self::AllOnes,
            _ => Self::Random,
        }
    }
}

impl AsRef<str> for RamState {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for RamState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::AllZeros => "All $00",
            Self::AllOnes => "All $FF",
            Self::Random => "Random",
        };
        write!(f, "{s}")
    }
}

impl FromStr for RamState {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all-zeros" => Ok(Self::AllZeros),
            "all-ones" => Ok(Self::AllOnes),
            "random" => Ok(Self::Random),
            _ => Err("invalid RamState value. valid options: `all-zeros`, `all-ones`, or `random`"),
        }
    }
}

/// Represents allowed [`Memory`] bank access.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum BankAccess {
    None,
    Read,
    ReadWrite,
}

/// Represents a set of [`Memory`] banks.
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Banks {
    start: usize,
    end: NonZeroUsize,
    size: usize,
    window: NonZeroUsize,
    pub(crate) shift: usize,
    pub(crate) mask: usize,
    pub(crate) banks: Vec<usize>,
    access: Vec<BankAccess>,
    page_count: usize,
}

#[derive(thiserror::Error, Debug)]
#[must_use]
pub enum Error {
    #[error("bank `{field}` must be non-zero.{context}")]
    Zero {
        field: &'static str,
        context: String,
    },
    #[error("bank `window` must be greater than total bank `size`")]
    InvalidWindow,
}

impl Banks {
    pub fn new(
        start: usize,
        end: impl TryInto<NonZeroUsize>,
        capacity: usize,
        window: impl TryInto<NonZeroUsize>,
    ) -> Result<Self, Error> {
        let end = end.try_into().map_err(|_| Error::Zero {
            field: "end",
            context: format!(" bank start: ${start:04X}"),
        })?;
        let window = window.try_into().map_err(|_| Error::Zero {
            field: "window",
            context: format!(" bank range: ${start:04X}..=${end:04X} (capacity: ${capacity:04X})"),
        })?;
        let mut size = end.get() - start;
        if size > capacity {
            size = capacity;
        }
        let bank_count = (size + 1) / window;

        let mut banks = vec![0; bank_count];
        let access = vec![BankAccess::ReadWrite; bank_count];
        for (i, bank) in banks.iter_mut().enumerate() {
            *bank = (i * window.get()) % capacity;
        }
        let page_count = capacity / window.get();

        Ok(Self {
            start,
            end,
            size,
            window,
            shift: window.trailing_zeros() as usize,
            mask: page_count.saturating_sub(1),
            banks,
            access,
            page_count,
        })
    }

    pub fn set(&mut self, mut bank: usize, page: usize) {
        if bank >= self.banks.len() {
            bank %= self.banks.len();
        }
        assert!(bank < self.banks.len());
        self.banks[bank] = (page & self.mask) << self.shift;
        debug_assert!(self.banks[bank] < self.page_count * self.window.get());
    }

    pub fn set_range(&mut self, start: usize, end: usize, page: usize) {
        let mut new_addr = (page & self.mask) << self.shift;
        for mut bank in start..=end {
            if bank >= self.banks.len() {
                bank %= self.banks.len();
            }
            assert!(bank < self.banks.len());
            self.banks[bank] = new_addr;
            debug_assert!(self.banks[bank] < self.page_count * self.window.get());
            new_addr += self.window.get();
        }
    }

    pub fn set_access(&mut self, mut bank: usize, access: BankAccess) {
        if bank >= self.banks.len() {
            bank %= self.banks.len();
        }
        assert!(bank < self.banks.len());
        self.access[bank] = access;
    }

    pub fn set_access_range(&mut self, start: usize, end: usize, access: BankAccess) {
        for slot in start..=end {
            self.set_access(slot, access);
        }
    }

    pub fn readable(&self, addr: u16) -> bool {
        let slot = self.get(addr);
        assert!(slot < self.banks.len());
        matches!(self.access[slot], BankAccess::Read | BankAccess::ReadWrite)
    }

    pub fn writable(&self, addr: u16) -> bool {
        let slot = self.get(addr);
        assert!(slot < self.banks.len());
        self.access[slot] == BankAccess::ReadWrite
    }

    #[must_use]
    pub const fn last(&self) -> usize {
        self.page_count.saturating_sub(1)
    }

    #[must_use]
    pub fn banks_len(&self) -> usize {
        self.banks.len()
    }

    #[must_use]
    pub const fn get(&self, addr: u16) -> usize {
        (addr as usize & self.size) >> self.shift
    }

    #[must_use]
    pub fn translate(&self, addr: u16) -> usize {
        let slot = self.get(addr);
        assert!(slot < self.banks.len());
        let page_offset = self.banks[slot];
        page_offset | (addr as usize) & (self.window.get() - 1)
    }

    #[must_use]
    pub fn page(&self, bank: usize) -> usize {
        self.banks[bank] >> self.shift
    }

    #[must_use]
    pub fn page_offset(&self, bank: usize) -> usize {
        self.banks[bank]
    }

    #[must_use]
    pub const fn page_count(&self) -> usize {
        self.page_count
    }
}

impl std::fmt::Debug for Banks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Bank")
            .field("start", &format_args!("${:04X}", self.start))
            .field("end", &format_args!("${:04X}", self.end))
            .field("size", &format_args!("${:04X}", self.size))
            .field("window", &format_args!("${:04X}", self.window))
            .field("shift", &self.shift)
            .field("mask", &self.shift)
            .field("banks", &self.banks)
            .field("page_count", &self.page_count)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_bank() {
        let banks = Banks::new(
            0x8000,
            NonZeroUsize::new(0xFFFF).unwrap(),
            128 * 1024,
            NonZeroUsize::new(0x4000).unwrap(),
        )
        .unwrap();
        assert_eq!(banks.get(0x8000), 0);
        assert_eq!(banks.get(0x9FFF), 0);
        assert_eq!(banks.get(0xA000), 0);
        assert_eq!(banks.get(0xBFFF), 0);
        assert_eq!(banks.get(0xC000), 1);
        assert_eq!(banks.get(0xDFFF), 1);
        assert_eq!(banks.get(0xE000), 1);
        assert_eq!(banks.get(0xFFFF), 1);
    }

    #[test]
    fn bank_translate() {
        let mut banks = Banks::new(
            0x8000,
            NonZeroUsize::new(0xFFFF).unwrap(),
            128 * 1024,
            NonZeroUsize::new(0x2000).unwrap(),
        )
        .unwrap();

        let last_bank = banks.last();
        assert_eq!(last_bank, 15, "bank count");

        assert_eq!(banks.translate(0x8000), 0x0000);
        banks.set(0, 1);
        assert_eq!(banks.translate(0x8000), 0x2000);
        banks.set(0, 2);
        assert_eq!(banks.translate(0x8000), 0x4000);
        banks.set(0, 0);
        assert_eq!(banks.translate(0x8000), 0x0000);
        banks.set(0, banks.last());
        assert_eq!(banks.translate(0x8000), 0x1E000);
    }
}
