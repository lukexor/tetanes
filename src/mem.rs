use rand::Rng;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Access {
    Read,
    Write,
    Execute,
    Dummy,
}

pub trait Mem {
    #[inline]
    fn read(&mut self, addr: u16, access: Access) -> u8 {
        self.peek(addr, access)
    }

    fn peek(&self, addr: u16, access: Access) -> u8;

    #[inline]
    fn read_u16(&mut self, addr: u16, access: Access) -> u16 {
        let lo = self.read(addr, access);
        let hi = self.read(addr.wrapping_add(1), access);
        u16::from_le_bytes([lo, hi])
    }

    #[inline]
    fn peek_u16(&self, addr: u16, access: Access) -> u16 {
        let lo = self.peek(addr, access);
        let hi = self.peek(addr.wrapping_add(1), access);
        u16::from_le_bytes([lo, hi])
    }

    fn write(&mut self, addr: u16, val: u8, access: Access);

    #[inline]
    fn write_u16(&mut self, addr: u16, val: u16, access: Access) {
        let [lo, hi] = val.to_le_bytes();
        self.write(addr, lo, access);
        self.write(addr, hi, access);
    }
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

    #[must_use]
    pub fn with_capacity(capacity: usize, state: Self) -> Vec<u8> {
        let mut ram = vec![0x00; capacity];
        Self::fill(&mut ram, state);
        ram
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
pub struct MemBanks {
    start: usize,
    end: usize,
    size: usize,
    window: usize,
    shift: usize,
    mask: usize,
    banks: Vec<usize>,
    page_count: usize,
}

impl MemBanks {
    pub fn new(start: usize, end: usize, capacity: usize, window: usize) -> Self {
        let size = end - start;
        let mut banks = vec![0; (size + 1) / window];
        for (i, bank) in banks.iter_mut().enumerate() {
            *bank = i * window;
        }
        let page_count = std::cmp::max(1, capacity / window);
        Self {
            start,
            end,
            size,
            window,
            shift: window.trailing_zeros() as usize,
            mask: page_count - 1,
            banks,
            page_count,
        }
    }

    #[inline]
    pub fn set(&mut self, slot: usize, bank: usize) {
        self.banks[slot] = (bank & self.mask) << self.shift;
        debug_assert!(self.banks[slot] < self.page_count * self.window);
    }

    #[inline]
    pub fn set_range(&mut self, start: usize, end: usize, bank: usize) {
        let mut new_addr = (bank & self.mask) << self.shift;
        for slot in start..=end {
            self.banks[slot] = new_addr;
            debug_assert!(self.banks[slot] < self.page_count * self.window);
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
    pub const fn get_bank(&self, addr: u16) -> usize {
        // 0x6005    - 0b0110000000000101 -> bank 0
        //  (0x2000)   0b0010000000000000
        //
        // 0x8005    - 0b1000000000000101 -> bank 0
        //   (0x4000)  0b0100000000000000
        // 0xC005    - 0b1100000000000101 -> bank 1
        //
        // 0x8005    - 0b1000000000000101 -> bank 0
        // 0xA005    - 0b1010000000000101 -> bank 1
        // 0xC005    - 0b1100000000000101 -> bank 2
        // 0xE005    - 0b1110000000000101 -> bank 3
        //   (0x2000)  0b0010000000000000
        ((addr as usize) & self.size) >> self.shift
    }

    #[inline]
    #[must_use]
    pub fn translate(&self, addr: u16) -> usize {
        // 0x6005    - 0b0110000000000101 -> bank 0
        //  (0x2000)   0b0010000000000000
        //
        // 0x8005    - 0b1000000000000101 -> bank 0
        //   (0x4000)  0b0100000000000000
        // 0xC005    - 0b1100000000000101 -> bank 1
        //
        // 0x8005    - 0b1000000000000101 -> bank 0
        //  0 -> 0x0000
        //  1
        //  2
        // 0xA005    - 0b1010000000000101 -> bank 1
        // 0xC005    - 0b1100000000000101 -> bank 2
        // 0xE005    - 0b1110000000000101 -> bank 3
        //   (0x2000)  0b0010000000000000
        let page = self.banks[self.get_bank(addr)];
        page | (addr as usize) & (self.window - 1)
    }
}

impl std::fmt::Debug for MemBanks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Bank")
            .field("start", &format_args!("0x{:04X}", self.start))
            .field("end", &format_args!("0x{:04X}", self.end))
            .field("size", &format_args!("0x{:04X}", self.size))
            .field("window", &format_args!("0x{:04X}", self.window))
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
        let size = 128 * 1024;
        let banks = MemBanks::new(0x8000, 0xFFFF, size, 0x4000);
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
        let mut banks = MemBanks::new(0x8000, 0xFFFF, size, 0x2000);

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
