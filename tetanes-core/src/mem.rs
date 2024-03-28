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
    pub fn with_capacity(capacity: usize, state: Self) -> Vec<u8> {
        let mut ram = vec![0x00; capacity];
        Self::fill(&mut ram, state);
        ram
    }

    pub const fn as_slice() -> &'static [Self] {
        &[Self::AllZeros, Self::AllOnes, Self::Random]
    }

    pub fn as_str(&self) -> &'static str {
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
        f.write_str(self.as_ref())
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

    pub fn set(&mut self, slot: usize, bank: usize) {
        assert!(slot < self.banks.len());
        self.banks[slot] = (bank & self.mask) << self.shift;
        debug_assert!(self.banks[slot] < self.page_count * self.window);
    }

    pub fn set_range(&mut self, start: usize, end: usize, bank: usize) {
        let mut new_addr = (bank & self.mask) << self.shift;
        for slot in start..=end {
            assert!(slot < self.banks.len());
            self.banks[slot] = new_addr;
            debug_assert!(self.banks[slot] < self.page_count * self.window);
            new_addr += self.window;
        }
    }

    #[must_use]
    pub const fn last(&self) -> usize {
        self.page_count.saturating_sub(1)
    }

    #[must_use]
    pub const fn get(&self, addr: u16) -> usize {
        // $6005    - 0b0110000000000101 -> bank 0
        //  ($2000)   0b0010000000000000
        //
        // $8005    - 0b1000000000000101 -> bank 0
        //   ($4000)  0b0100000000000000
        // $C005    - 0b1100000000000101 -> bank 1
        //
        // $8005    - 0b1000000000000101 -> bank 0
        // $A005    - 0b1010000000000101 -> bank 1
        // $C005    - 0b1100000000000101 -> bank 2
        // $E005    - 0b1110000000000101 -> bank 3
        //   ($2000)  0b0010000000000000
        ((addr as usize) & self.size) >> self.shift
    }

    #[must_use]
    pub fn translate(&self, addr: u16) -> usize {
        // $6005    - 0b0110000000000101 -> bank 0
        //  ($2000)   0b0010000000000000
        //
        // $8005    - 0b1000000000000101 -> bank 0
        //   ($4000)  0b0100000000000000
        // $C005    - 0b1100000000000101 -> bank 1
        //
        // $8005    - 0b1000000000000101 -> bank 0
        //  0 -> $0000
        //  1
        //  2
        // $A005    - 0b1010000000000101 -> bank 1
        // $C005    - 0b1100000000000101 -> bank 2
        // $E005    - 0b1110000000000101 -> bank 3
        //   ($2000)  0b0010000000000000
        let slot = self.get(addr);
        assert!(slot < self.banks.len());
        let page = self.banks[slot];
        page | (addr as usize) & (self.window - 1)
    }
}

impl std::fmt::Debug for MemBanks {
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
        let size = 128 * 1024;
        let banks = MemBanks::new(0x8000, 0xFFFF, size, 0x4000);
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
