//! Memory types for dealing with bytes

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    ops::{Deref, DerefMut},
    str::FromStr,
};

pub trait MemRead {
    #[inline]
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }
    #[inline]
    fn peek(&self, addr: u16) -> u8 {
        self.peekw(addr as usize)
    }
    #[inline]
    fn readw(&mut self, addr: usize) -> u8 {
        self.peekw(addr)
    }
    fn peekw(&self, _addr: usize) -> u8 {
        0x00
    }
}

pub trait MemWrite {
    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        self.writew(addr as usize, val);
    }
    fn writew(&mut self, _addr: usize, _val: u8) {}
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum MemAccess {
    Read,
    Write,
    Execute,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum RamState {
    AllZeros,
    AllOnes,
    Random,
}

impl RamState {
    pub const fn as_slice() -> &'static [Self] {
        &[Self::AllZeros, Self::AllOnes, Self::Random]
    }
}

impl Default for RamState {
    fn default() -> Self {
        Self::AllZeros
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
        match self {
            Self::AllZeros => "All $00",
            Self::AllOnes => "All $FF",
            Self::Random => "Random",
        }
    }
}

impl FromStr for RamState {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all_zeros" => Ok(Self::AllZeros),
            "all_ones" => Ok(Self::AllOnes),
            "random" => Ok(Self::Random),
            _ => Err("invalid RamState value. valid options: `all_zeros`, `all_ones`, or `random`"),
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Memory {
    data: Vec<u8>,
    writable: bool,
    state: RamState,
}

impl Memory {
    #[inline]
    pub const fn new() -> Self {
        Self {
            data: vec![],
            writable: true,
            state: RamState::AllZeros,
        }
    }

    #[inline]
    pub fn rom(bytes: Vec<u8>) -> Self {
        Self {
            data: bytes,
            writable: false,
            state: RamState::AllZeros,
        }
    }

    #[inline]
    pub fn ram(capacity: usize, state: RamState) -> Self {
        Self {
            data: Self::allocate_ram(capacity, state),
            writable: true,
            state,
        }
    }

    #[inline]
    pub fn load(&mut self, mut bytes: Vec<u8>) {
        self.data.clear();
        self.data.append(&mut bytes);
    }

    #[inline]
    pub fn resize(&mut self, new_size: usize) {
        self.data = Self::allocate_ram(new_size, self.state);
    }

    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[must_use]
    #[inline]
    pub const fn writable(&self) -> bool {
        self.writable
    }

    #[inline]
    pub fn write_protect(&mut self, protect: bool) {
        self.writable = !protect;
    }

    #[inline]
    fn allocate_ram(capacity: usize, state: RamState) -> Vec<u8> {
        match state {
            RamState::AllZeros => vec![0x00; capacity],
            RamState::AllOnes => vec![0xFF; capacity],
            RamState::Random => {
                let mut rng = rand::thread_rng();
                let mut data = Vec::with_capacity(capacity);
                for _ in 0..capacity {
                    data.push(rng.gen_range(0x00..=0xFF));
                }
                data
            }
        }
    }
}

impl MemRead for Memory {
    #[inline]
    fn peekw(&self, addr: usize) -> u8 {
        let len = self.data.len();
        debug_assert!(len > 0, "${:04X}: {:?}", addr, &self);
        self.data[addr % len]
    }
}

impl MemWrite for Memory {
    #[inline]
    fn writew(&mut self, addr: usize, val: u8) {
        let len = self.data.len();
        debug_assert!(len > 0, "${:04X} -> ${:02X}: {:?}", addr, val, &self);
        if self.writable {
            self.data[addr % len] = val;
        }
    }
}

impl Deref for Memory {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for Memory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl From<Vec<u8>> for Memory {
    fn from(data: Vec<u8>) -> Self {
        Self {
            data,
            writable: true,
            state: RamState::default(),
        }
    }
}

impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Memory")
            .field("data", &format_args!("{} KB", self.data.len() / 1024))
            .field("writable", &self.writable)
            .finish()
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[must_use]
pub struct MemoryBanks {
    start: usize,
    end: usize,
    window: usize,
    banks: Vec<usize>,
    page_count: usize,
}

impl MemoryBanks {
    #[inline]
    pub fn new(start: usize, end: usize, capacity: usize, window: usize) -> Self {
        let bank_count = (end - start + 1) / window;
        let mut banks = Vec::with_capacity(bank_count);
        for bank in 0..bank_count {
            banks.push(bank * window);
        }
        Self {
            start,
            end,
            window,
            banks,
            page_count: std::cmp::max(1, capacity / window),
        }
    }

    #[inline]
    pub fn set(&mut self, slot: usize, mut bank: usize) {
        if bank >= self.page_count {
            bank %= self.page_count;
        }
        self.banks[slot] = bank * self.window;
    }

    #[inline]
    pub fn set_range(&mut self, start: usize, end: usize, mut bank: usize) {
        if bank >= self.page_count {
            bank %= self.page_count;
        }
        let mut new_addr = bank * self.window;
        for slot in start..=end {
            self.banks[slot] = new_addr;
            new_addr += self.window;
        }
    }

    #[inline]
    #[must_use]
    pub const fn last(&self) -> usize {
        self.page_count.saturating_sub(1)
    }

    #[inline]
    #[must_use]
    pub fn get_bank(&self, addr: u16) -> usize {
        ((addr as usize - self.start) >> self.window.trailing_zeros() as usize)
            & (self.banks.len() - 1)
    }

    #[inline]
    #[must_use]
    pub fn translate(&self, addr: u16) -> usize {
        let bank = self.get_bank(addr);
        let offset = (addr as usize) & (self.window - 1);
        self.banks[bank] + offset
    }
}

impl fmt::Debug for MemoryBanks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Bank")
            .field("start", &format_args!("0x{:04X}", self.start))
            .field("end", &format_args!("0x{:04X}", self.end))
            .field("window", &format_args!("0x{:04X}", self.window))
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
        let size = 128 * 1024;
        let banks = MemoryBanks::new(0x8000, 0xFFFF, size, 0x4000);
        assert_eq!(banks.get_bank(0x8000), 0);
        assert_eq!(banks.get_bank(0x9FFF), 0);
        assert_eq!(banks.get_bank(0xA000), 0);
        assert_eq!(banks.get_bank(0xBFFF), 0);
        assert_eq!(banks.get_bank(0xC000), 1);
        assert_eq!(banks.get_bank(0xDFFF), 1);
        assert_eq!(banks.get_bank(0xE000), 1);
        assert_eq!(banks.get_bank(0xFFFF), 1);
    }

    #[test]
    fn bank_translate() {
        let size = 128 * 1024;
        let mut banks = MemoryBanks::new(0x8000, 0xFFFF, size, 0x2000);

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
