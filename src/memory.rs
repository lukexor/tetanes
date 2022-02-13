//! Memory types for dealing with bytes

use crate::{
    common::{Addr, Byte},
    mapper::MapperType,
    serialization::Savable,
    NesResult,
};
use enum_dispatch::enum_dispatch;
use rand::Rng;
use std::{
    fmt,
    io::{Read, Write},
    ops::{Deref, DerefMut},
};

#[enum_dispatch(MapperType)]
pub trait MemRead {
    fn read(&mut self, _addr: Addr) -> Byte {
        0
    }

    fn peek(&self, _addr: Addr) -> Byte {
        0
    }
}

#[enum_dispatch(MapperType)]
pub trait MemWrite {
    fn write(&mut self, _addr: Addr, _val: Byte) {}
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum MemAccess {
    Read,
    Write,
}

#[derive(Default, Clone)]
#[must_use]
pub struct Memory {
    data: Vec<Byte>,
    writable: bool,
}

impl Memory {
    pub fn new(consistent: bool) -> Self {
        Self::with_capacity(0, consistent)
    }

    pub fn with_capacity(capacity: usize, consistent: bool) -> Self {
        let data = if consistent {
            vec![0; capacity]
        } else {
            let mut rng = rand::thread_rng();
            let mut data = Vec::with_capacity(capacity);
            for _ in 0..capacity {
                data.push(rng.gen_range(0x00..=0xFF));
            }
            data
        };
        Self {
            data,
            writable: true,
        }
    }

    pub fn from_bytes(bytes: &[Byte]) -> Self {
        let consistent = true;
        let mut memory = Self::with_capacity(bytes.len(), consistent);
        memory.data = bytes.to_vec();
        memory
    }

    pub fn rom(capacity: usize) -> Self {
        let consistent = true;
        let mut rom = Self::with_capacity(capacity, consistent);
        rom.writable = false;
        rom
    }

    pub fn rom_from_bytes(bytes: &[Byte]) -> Self {
        let mut rom = Self::rom(bytes.len());
        rom.data = bytes.to_vec();
        rom
    }

    pub fn ram(capacity: usize, consistent: bool) -> Self {
        Self::with_capacity(capacity, consistent)
    }

    pub fn ram_from_bytes(bytes: &[Byte]) -> Self {
        let consistent = true;
        let mut ram = Self::ram(bytes.len(), consistent);
        ram.data = bytes.to_vec();
        ram
    }

    #[inline]
    pub fn extend(&mut self, memory: &Memory) {
        self.data.extend(&memory.data);
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
}

impl MemRead for Memory {
    #[inline]
    fn read(&mut self, addr: Addr) -> Byte {
        self.peek(addr)
    }

    fn peek(&self, addr: Addr) -> Byte {
        let addr = addr as usize % self.len();
        self.data[addr]
    }
}

impl MemWrite for Memory {
    fn write(&mut self, addr: Addr, val: Byte) {
        if self.writable {
            let addr = addr as usize % self.len();
            self.data[addr] = val;
        }
    }
}

impl Savable for Memory {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.data.save(fh)?;
        self.writable.save(fh)?;
        Ok(())
    }

    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.data.load(fh)?;
        self.writable.load(fh)?;
        Ok(())
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

impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Memory")
            .field("data", &format_args!("{} KB", self.data.len() / 1024))
            .field("writable", &self.writable)
            .finish()
    }
}

#[derive(Default, Clone)]
#[must_use]
struct Bank {
    start: usize,
    end: usize,
    address: usize,
}

impl Bank {
    const fn new(start: Addr, end: Addr) -> Self {
        Self {
            start: start as usize,
            end: end as usize,
            address: start as usize,
        }
    }
}

impl Savable for Bank {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.start.save(fh)?;
        self.end.save(fh)?;
        self.address.save(fh)?;
        Ok(())
    }

    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.start.load(fh)?;
        self.end.load(fh)?;
        self.address.load(fh)?;
        Ok(())
    }
}

impl fmt::Debug for Bank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Bank")
            .field("start", &format_args!("0x{:04X}", self.start))
            .field("end", &format_args!("0x{:04X}", self.end))
            .field("address", &format_args!("0x{:04X}", self.address))
            .finish()
    }
}

#[derive(Default, Clone)]
#[must_use]
pub struct BankedMemory {
    banks: Vec<Bank>,
    window: usize,
    bank_shift: usize,
    bank_count: usize,
    memory: Memory,
}

impl BankedMemory {
    pub fn new(window: usize, consistent: bool) -> Self {
        Self::ram(0x2000, window, consistent)
    }

    pub fn ram(capacity: usize, window: usize, consistent: bool) -> Self {
        let memory = Memory::ram(capacity, consistent);
        Self {
            banks: Vec::new(),
            window,
            bank_shift: Self::bank_shift(window),
            bank_count: std::cmp::max(1, memory.len() / window),
            memory,
        }
    }

    pub fn from(memory: Memory, window: usize) -> Self {
        Self {
            banks: Vec::new(),
            window,
            bank_shift: Self::bank_shift(window),
            bank_count: std::cmp::max(1, memory.len() / window),
            memory,
        }
    }

    pub fn extend(&mut self, memory: &Memory) {
        self.memory.extend(memory);
        self.bank_count = std::cmp::max(1, self.memory.len() / self.window);
    }

    pub fn add_bank(&mut self, start: Addr, end: Addr) {
        self.banks.push(Bank::new(start, end));
        self.update_banks();
    }

    pub fn add_bank_range(&mut self, start: Addr, end: Addr) {
        for start in (start..end).step_by(self.window) {
            let end = start + (self.window as Addr).saturating_sub(1);
            self.banks.push(Bank::new(start, end));
        }
        self.update_banks();
    }

    pub fn set_bank(&mut self, bank_start: Addr, new_bank: usize) {
        let bank = self.get_bank(bank_start);
        debug_assert!(
            bank < self.banks.len(),
            "bank is outside bankable range {} / {}",
            bank,
            self.banks.len()
        );
        self.banks[bank].address = (new_bank % self.bank_count()) * self.window;
    }

    pub fn set_bank_range(&mut self, start: Addr, end: Addr, new_bank: usize) {
        let mut new_address = (new_bank % self.bank_count()) * self.window;
        for bank_start in (start..end).step_by(self.window) {
            let bank = self.get_bank(bank_start);
            debug_assert!(
                bank < self.banks.len(),
                "bank is outside bankable range {} / {}",
                bank,
                self.banks.len()
            );
            self.banks[bank].address = new_address;
            new_address += self.window;
        }
    }

    #[inline]
    pub fn set_bank_mirror(&mut self, bank_start: Addr, mirror_bank: usize) {
        self.set_bank(bank_start, mirror_bank);
    }

    #[must_use]
    #[inline]
    pub const fn last_bank(&self) -> usize {
        self.bank_count.saturating_sub(1)
    }

    #[must_use]
    #[inline]
    pub const fn bank_count(&self) -> usize {
        self.bank_count
    }

    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.memory.len()
    }

    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.memory.is_empty()
    }

    #[must_use]
    #[inline]
    pub const fn writable(&self) -> bool {
        self.memory.writable()
    }

    #[inline]
    pub fn write_protect(&mut self, protect: bool) {
        self.memory.write_protect(protect);
    }

    fn update_banks(&mut self) {
        self.banks.sort_by(|a, b| a.start.cmp(&b.start));
        let mut address = 0x0000;
        for bank in &mut self.banks {
            bank.address = address;
            address += bank.end - bank.start + 1;
        }
    }

    #[must_use]
    pub fn get_bank(&self, addr: Addr) -> usize {
        let addr = addr as usize;
        let base_addr = self.banks.first().map_or(0x0000, |bank| bank.start);
        debug_assert!(addr >= base_addr, "address is less than base address");
        ((addr - base_addr) >> self.bank_shift) % self.bank_count()
    }

    #[must_use]
    pub fn translate_addr(&self, addr: Addr) -> usize {
        let bank = self.get_bank(addr);
        debug_assert!(bank < self.banks.len(), "bank is outside bankable range");
        let bank = &self.banks[bank];
        bank.address + (addr as usize - bank.start)
    }

    #[must_use]
    const fn bank_shift(mut window: usize) -> usize {
        let mut shift = 0usize;
        while window > 0 {
            window >>= 1;
            shift += 1;
        }
        shift.saturating_sub(1)
    }
}

impl MemRead for BankedMemory {
    #[inline]
    fn read(&mut self, addr: Addr) -> Byte {
        self.peek(addr)
    }

    fn peek(&self, addr: Addr) -> Byte {
        let addr = self.translate_addr(addr) % self.len();
        self.memory[addr]
    }
}

impl MemWrite for BankedMemory {
    fn write(&mut self, addr: Addr, val: Byte) {
        if self.writable() {
            let addr = self.translate_addr(addr) % self.len();
            self.memory[addr] = val;
        }
    }
}

impl Savable for BankedMemory {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.banks.save(fh)?;
        self.window.save(fh)?;
        self.bank_shift.save(fh)?;
        self.bank_count.save(fh)?;
        self.memory.save(fh)?;
        Ok(())
    }

    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.banks.load(fh)?;
        self.window.load(fh)?;
        self.bank_shift.load(fh)?;
        self.bank_count.load(fh)?;
        self.memory.load(fh)?;
        Ok(())
    }
}

impl Deref for BankedMemory {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.memory
    }
}

impl DerefMut for BankedMemory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.memory
    }
}

impl fmt::Debug for BankedMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("BankedMemory")
            .field("banks", &self.banks)
            .field("window", &format_args!("0x{:04X}", self.window))
            .field("bank_shift", &self.bank_shift)
            .field("bank_count", &self.bank_count)
            .field("memory", &self.memory)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CONSISTENT_RAM: bool = true;

    #[test]
    fn add_bank_range_test() {
        let mut memory = BankedMemory::ram(0xFFFF, 0x2000, CONSISTENT_RAM);
        memory.add_bank_range(0x6000, 0xFFFF);
        assert_eq!(memory.get_bank(0x6000), 0);
        assert_eq!(memory.get_bank(0x7FFF), 0);
        assert_eq!(memory.get_bank(0x8000), 1);
        assert_eq!(memory.get_bank(0x9FFF), 1);
        assert_eq!(memory.get_bank(0xA000), 2);
        assert_eq!(memory.get_bank(0xBFFF), 2);
        assert_eq!(memory.get_bank(0xC000), 3);
        assert_eq!(memory.get_bank(0xDFFF), 3);
        assert_eq!(memory.get_bank(0xE000), 4);
        assert_eq!(memory.get_bank(0xFFFF), 4);

        let mut memory = BankedMemory::ram(0xFFFF, 0x2000, CONSISTENT_RAM);
        memory.add_bank_range(0x8000, 0xBFFF);
        assert_eq!(memory.get_bank(0x8000), 0);
        assert_eq!(memory.get_bank(0x9FFF), 0);
        assert_eq!(memory.get_bank(0xA000), 1);
        assert_eq!(memory.get_bank(0xBFFF), 1);

        memory.add_bank(0x6000, 0x7FFF);
        assert_eq!(memory.get_bank(0x6000), 0);
        assert_eq!(memory.get_bank(0x8000), 1);

        let mut memory = BankedMemory::ram(0xFFFF, 0x0400, CONSISTENT_RAM);
        memory.add_bank_range(0x0000, 0x1FFF);
        assert_eq!(memory.get_bank(0x0000), 0);
        assert_eq!(memory.get_bank(0x03FF), 0);
        assert_eq!(memory.get_bank(0x0400), 1);
        assert_eq!(memory.get_bank(0x07FF), 1);
        assert_eq!(memory.get_bank(0x1C00), 7);
        assert_eq!(memory.get_bank(0x1FFF), 7);
    }

    #[test]
    fn add_bank_test() {
        let mut memory = BankedMemory::ram(0xFFFF, 0x4000, CONSISTENT_RAM);
        memory.add_bank(0x8000, 0xBFFF);
        memory.add_bank(0xC000, 0xFFFF);
        assert_eq!(memory.get_bank(0x8000), 0);
        assert_eq!(memory.get_bank(0x9FFF), 0);
        assert_eq!(memory.get_bank(0xA000), 0);
        assert_eq!(memory.get_bank(0xBFFF), 0);
        assert_eq!(memory.get_bank(0xC000), 1);
        assert_eq!(memory.get_bank(0xDFFF), 1);
        assert_eq!(memory.get_bank(0xE000), 1);
        assert_eq!(memory.get_bank(0xFFFF), 1);
    }

    #[test]
    fn peek_bank_test() {
        let size = 40 * 1024;
        let rom = Memory::ram(size, CONSISTENT_RAM);
        let mut memory = BankedMemory::from(rom, 0x2000);

        assert!(!memory.is_empty(), "memory non-empty");
        assert_eq!(memory.len(), size, "memory size");

        memory.add_bank_range(0x8000, 0xFFFF);
        memory.memory.write(0x0000, 1);
        memory.memory.write(0x0001, 2);
        memory.memory.write(0x2000, 3);
        memory.memory.write(0x2001, 4);
        memory.memory.write(0x4000, 5);
        memory.memory.write(0x4001, 6);
        memory.memory.write(0x6000, 7);
        memory.memory.write(0x6001, 8);

        assert_eq!(memory.peek(0x8000), 1);
        assert_eq!(memory.peek(0x8001), 2);
        assert_eq!(memory.peek(0xA000), 3);
        assert_eq!(memory.peek(0xA001), 4);
        assert_eq!(memory.peek(0xC000), 5);
        assert_eq!(memory.peek(0xC001), 6);
        assert_eq!(memory.peek(0xE000), 7);
        assert_eq!(memory.peek(0xE001), 8);
    }

    #[test]
    fn write_bank_test() {
        let size = 40 * 1024;
        let rom = Memory::ram(size, CONSISTENT_RAM);
        let mut memory = BankedMemory::from(rom, 0x2000);

        assert!(!memory.is_empty(), "memory non-empty");
        assert_eq!(memory.len(), size, "memory size");

        memory.add_bank_range(0x8000, 0xFFFF);
        memory.write(0x8000, 11);
        memory.write(0xA000, 22);
        memory.write(0xC000, 33);
        memory.write(0xE000, 44);

        assert_eq!(memory.memory.peek(0x0000), 11);
        assert_eq!(memory.memory.peek(0x2000), 22);
        assert_eq!(memory.memory.peek(0x4000), 33);
        assert_eq!(memory.memory.peek(0x6000), 44);

        memory.write_protect(true);
        memory.write(0x8000, 255);
        assert_eq!(memory.memory.peek(0x0000), 11);
    }

    #[test]
    fn set_bank_test() {
        let size = 128 * 1024;
        let rom = Memory::ram(size, CONSISTENT_RAM);
        let mut memory = BankedMemory::from(rom, 0x2000);

        assert!(!memory.is_empty(), "memory non-empty");
        assert_eq!(memory.len(), size, "memory size");
        let last_bank = memory.last_bank();
        assert_eq!(last_bank, 15, "bank count");

        memory.add_bank_range(0x8000, 0xFFFF);
        memory.write(0x8000, 11);
        memory.write(0xA000, 22);
        assert_eq!(memory.peek(0x8000), 11);

        memory.set_bank(0x8000, 1);
        assert_eq!(memory.peek(0x8000), 22);

        memory.write(0xA000, 33);
        assert_eq!(memory.peek(0x8000), 33);

        memory.set_bank(0x8000, 0);
        assert_eq!(memory.peek(0x8000), 11);

        memory.set_bank(0x8000, last_bank);
        memory.write(0x8000, 255);
        assert_eq!(memory.peek(0x8000), 255);
    }

    #[test]
    fn bank_mirroring_test() {
        pretty_env_logger::init_timed();

        let size = 128 * 1024;
        let rom = Memory::ram(size, CONSISTENT_RAM);
        let mut memory = BankedMemory::from(rom, 0x4000);

        assert!(!memory.is_empty(), "memory non-empty");
        assert_eq!(memory.len(), size, "memory size");

        memory.add_bank_range(0x8000, 0xFFFF);
        memory.set_bank_mirror(0xC000, 0);

        memory.write(0x8000, 11);
        memory.write(0xA000, 22);

        assert_eq!(memory.peek(0x8000), 11);
        assert_eq!(memory.peek(0xA000), 22);
        assert_eq!(memory.peek(0xC000), 11);
        assert_eq!(memory.peek(0xE000), 22);
    }
}
