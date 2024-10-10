//! Memory and Bankswitching implementations.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{num::NonZeroUsize, str::FromStr};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Access {
    Read,
    Write,
    Execute,
    Dummy,
}

pub trait Mem {
    fn read(&mut self, addr: u16, access: Access) -> u8 {
        self.peek(addr, access)
    }

    fn peek(&self, addr: u16, access: Access) -> u8;

    fn read_u16(&mut self, addr: u16, access: Access) -> u16 {
        let lo = self.read(addr, access);
        let hi = self.read(addr.wrapping_add(1), access);
        u16::from_le_bytes([lo, hi])
    }

    fn peek_u16(&self, addr: u16, access: Access) -> u16 {
        let lo = self.peek(addr, access);
        let hi = self.peek(addr.wrapping_add(1), access);
        u16::from_le_bytes([lo, hi])
    }

    fn write(&mut self, addr: u16, val: u8, access: Access);

    fn write_u16(&mut self, addr: u16, val: u16, access: Access) {
        let [lo, hi] = val.to_le_bytes();
        self.write(addr, lo, access);
        self.write(addr, hi, access);
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum RamState {
    #[default]
    AllZeros,
    AllOnes,
    Random,
}

impl RamState {
    #[must_use]
    pub fn filled(capacity: usize, state: Self) -> Vec<u8> {
        let mut ram = vec![0x00; capacity];
        Self::fill(&mut ram, state);
        ram
    }

    pub const fn as_slice() -> &'static [Self] {
        &[Self::AllZeros, Self::AllOnes, Self::Random]
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AllZeros => "all-zeros",
            Self::AllOnes => "all-ones",
            Self::Random => "random",
        }
    }

    pub fn fill(ram: &mut [u8], state: RamState) {
        match state {
            RamState::AllZeros => ram.fill(0x00),
            RamState::AllOnes => ram.fill(0xFF),
            RamState::Random => {
                let mut rng = rand::thread_rng();
                for val in ram {
                    *val = rng.gen_range(0x00..=0xFF);
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

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Banks {
    start: usize,
    end: NonZeroUsize,
    size: NonZeroUsize,
    window: NonZeroUsize,
    shift: usize,
    mask: usize,
    banks: Vec<usize>,
    page_count: NonZeroUsize,
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
        capacity: impl TryInto<NonZeroUsize>,
        window: impl TryInto<NonZeroUsize>,
    ) -> Result<Self, Error> {
        let end = end.try_into().map_err(|_| Error::Zero {
            field: "end",
            context: format!(" bank start: ${start:04X}"),
        })?;
        let capacity = capacity.try_into().map_err(|_| Error::Zero {
            field: "capacity",
            context: format!(" bank range: ${start:04X}..=${end:04X}"),
        })?;
        let window = window.try_into().map_err(|_| Error::Zero {
            field: "window",
            context: format!(" bank range: ${start:04X}..=${end:04X} (capacity: ${capacity:04X})"),
        })?;
        let size = NonZeroUsize::new(end.get() - start).ok_or(Error::Zero {
            field: "size",
            context: format!(
                "  bank range: ${start:04X}..=${end:04X} (capacity: ${capacity:04X}, window: ${window:04X})"
            ),
        })?;
        let bank_count =
            NonZeroUsize::new((size.get() + 1) / window).ok_or(Error::InvalidWindow)?;

        let mut banks = vec![0; bank_count.get()];
        for (i, bank) in banks.iter_mut().enumerate() {
            *bank = i * window.get();
        }
        // If capacity < window, clamp page_count to 1
        let page_count =
            NonZeroUsize::new(capacity.get() / window.get()).unwrap_or(NonZeroUsize::MIN);

        Ok(Self {
            start,
            end,
            size,
            window,
            shift: window.trailing_zeros() as usize,
            mask: page_count.get() - 1,
            banks,
            page_count,
        })
    }

    pub fn set(&mut self, slot: usize, bank: usize) {
        assert!(slot < self.banks.len());
        self.banks[slot] = (bank & self.mask) << self.shift;
        debug_assert!(self.banks[slot] < self.page_count.get() * self.window.get());
    }

    pub fn set_range(&mut self, start: usize, end: impl TryInto<NonZeroUsize>, bank: usize) {
        let Ok(end) = end.try_into() else {
            tracing::warn!("invalid bank range: `end` must be non-zero");
            return;
        };

        let mut new_addr = (bank & self.mask) << self.shift;
        for slot in start..=end.get() {
            assert!(slot < self.banks.len());
            self.banks[slot] = new_addr;
            debug_assert!(self.banks[slot] < self.page_count.get() * self.window.get());
            new_addr += self.window.get();
        }
    }

    #[must_use]
    pub const fn last(&self) -> usize {
        self.page_count.get() - 1
    }

    #[must_use]
    pub const fn get(&self, addr: u16) -> usize {
        ((addr as usize) & self.size.get()) >> self.shift
    }

    #[must_use]
    pub fn translate(&self, addr: u16) -> usize {
        let slot = self.get(addr);
        assert!(slot < self.banks.len());
        let page = self.banks[slot];
        page | (addr as usize) & (self.window.get() - 1)
    }

    #[must_use]
    pub fn page(&self, slot: usize) -> usize {
        self.banks[slot]
    }

    #[must_use]
    pub const fn page_count(&self) -> NonZeroUsize {
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
        let size = NonZeroUsize::new(128 * 1024).unwrap();
        let banks = Banks::new(
            0x8000,
            NonZeroUsize::new(0xFFFF).unwrap(),
            size,
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
        let size = NonZeroUsize::new(128 * 1024).unwrap();
        let mut banks = Banks::new(
            0x8000,
            NonZeroUsize::new(0xFFFF).unwrap(),
            size,
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
