//! Memory and Bankswitching implementations.tetanes-core/src/mem.rs

use crate::common::{Reset, ResetKind};
use rand::Rng;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{SeqAccess, Visitor},
    ser::SerializeTuple,
};
use std::{
    fmt,
    marker::PhantomData,
    num::NonZeroUsize,
    ops::{Deref, DerefMut},
    str::FromStr,
};

/// Represents stack ROM or RAM memory in bytes, with a custom Debug implementation that avoids
/// printing the entire contents.
#[derive(Clone)]
pub struct ConstMemory<T, const N: usize> {
    ram_state: RamState,
    data: [T; N],
}

impl<T, const N: usize> Default for ConstMemory<T, N>
where
    T: Default + Copy,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> ConstMemory<T, N> {
    /// Create a new, empty `StaticMemory` instance.
    pub fn new() -> Self
    where
        T: Default + Copy,
    {
        Self {
            ram_state: RamState::AllZeros,
            data: [T::default(); N],
        }
    }
}

impl<const N: usize> ConstMemory<u8, N> {
    /// Fill ram based on [`RamState`].
    pub fn with_ram_state(mut self, state: RamState) -> Self {
        self.ram_state = state;
        self.ram_state.fill(&mut self.data);
        self
    }
}

impl<const N: usize> Reset for ConstMemory<u8, N> {
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            self.ram_state.fill(&mut self.data);
        }
    }
}

impl<T, const N: usize> fmt::Debug for ConstMemory<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticMemory")
            .field("len", &self.data.len())
            .field("ram_state", &self.ram_state)
            .finish()
    }
}

impl<T, const N: usize> Deref for ConstMemory<T, N> {
    type Target = [T; N];
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T, const N: usize> DerefMut for ConstMemory<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T: Serialize, const N: usize> Serialize for ConstMemory<T, N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_tuple(N)?;
        for item in &self.data {
            s.serialize_element(item)?;
        }
        s.end()
    }
}

impl<'de, T, const N: usize> Deserialize<'de> for ConstMemory<T, N>
where
    T: Deserialize<'de> + Default + Copy,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ArrayVisitor<T, const N: usize>(PhantomData<T>);

        impl<'de, T, const N: usize> Visitor<'de> for ArrayVisitor<T, N>
        where
            T: Deserialize<'de> + Default + Copy,
        {
            type Value = [T; N];

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&format!("an array of length {}", N))
            }

            #[inline]
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut data = [T::default(); N];
                for data in &mut data {
                    match (seq.next_element())? {
                        Some(val) => *data = val,
                        None => return Err(serde::de::Error::invalid_length(N, &self)),
                    }
                }
                Ok(data)
            }
        }

        deserializer
            .deserialize_tuple(N, ArrayVisitor(PhantomData))
            .map(|data| Self {
                ram_state: RamState::default(),
                data,
            })
    }
}

/// Represents dynamic ROM or RAM memory in bytes, with a custom Debug implementation that avoids
/// printing the entire contents.
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct DynMemory<T> {
    ram_state: RamState,
    data: Vec<T>,
}

impl<T> DynMemory<T> {
    /// Create a new, empty `Memory` instance.
    pub const fn new() -> Self {
        Self {
            ram_state: RamState::AllZeros,
            data: Vec::new(),
        }
    }

    /// Create a new `Memory` instance of a given size, zeroed out.
    pub fn with_size(size: usize) -> Self
    where
        T: Default + Copy,
    {
        Self {
            ram_state: RamState::default(),
            data: vec![T::default(); size],
        }
    }
}

impl DynMemory<u8> {
    /// Fill ram based on [`RamState`].
    pub fn with_ram_state(mut self, state: RamState, size: usize) -> Self {
        self.ram_state = state;
        self.resize(size);
        self
    }

    pub fn resize(&mut self, size: usize) {
        self.data.resize(size, 0);
        self.ram_state.fill(&mut self.data);
    }
}

impl<T> Deref for DynMemory<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for DynMemory<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl From<Vec<u8>> for DynMemory<u8> {
    fn from(data: Vec<u8>) -> Self {
        Self {
            ram_state: RamState::default(),
            data,
        }
    }
}

impl<T> fmt::Debug for DynMemory<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynMemory")
            .field("len", &self.data.len())
            .field("capacity", &self.data.capacity())
            .field("ram_state", &self.ram_state)
            .finish()
    }
}

impl Reset for DynMemory<u8> {
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            self.ram_state.fill(&mut self.data);
        }
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

    /// Fills data slice based on `RamState`.
    pub fn fill(&self, data: &mut [u8]) {
        match self {
            RamState::AllZeros => data.fill(0x00),
            RamState::AllOnes => data.fill(0xFF),
            RamState::Random => {
                let mut rng = rand::rng();
                for val in data {
                    *val = rng.random_range(0x00..=0xFF);
                }
            }
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
    end: usize,
    size: NonZeroUsize,
    window: NonZeroUsize,
    shift: usize,
    mask: usize,
    banks: Vec<usize>,
    access: Vec<BankAccess>,
    page_count: usize,
}

#[derive(thiserror::Error, Debug)]
#[must_use]
pub enum Error {
    #[error("bank `window` must a non-zero power of two")]
    InvalidWindow,
    #[error("bank `size` must be non-zero")]
    InvalidSize,
}

impl Banks {
    pub fn new(
        start: usize,
        end: usize,
        capacity: usize,
        window: impl TryInto<NonZeroUsize>,
    ) -> Result<Self, Error> {
        let window = window.try_into().map_err(|_| Error::InvalidWindow)?;
        if !window.is_power_of_two() {
            return Err(Error::InvalidWindow);
        }

        let size = NonZeroUsize::try_from(end - start).map_err(|_| Error::InvalidSize)?;
        let bank_count = (size.get() + 1) / window;

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
        (addr as usize & self.size.get()) >> self.shift
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
            0xFFFF,
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
            0xFFFF,
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
