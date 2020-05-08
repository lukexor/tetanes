//! Memory types for dealing with bytes

use crate::{
    common::{Addr, Byte, Word},
    mapper::*,
    serialization::Savable,
    NesResult,
};
use enum_dispatch::enum_dispatch;
use log::debug;
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
    fn readw(&mut self, _addr: Word) -> Byte {
        0
    }
    fn peek(&self, _addr: Addr) -> Byte {
        0
    }
    fn peekw(&self, _addr: Word) -> Byte {
        0
    }
}
#[enum_dispatch(MapperType)]
pub trait MemWrite {
    fn write(&mut self, _addr: Addr, _val: Byte) {}
    fn writew(&mut self, _addr: Word, _val: Byte) {}
}
pub trait Bankable {
    type Item;
    fn chunks(&self, size: usize) -> Vec<Self::Item>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

#[derive(Default, Clone)]
pub struct Memory {
    data: Vec<Byte>,
    writable: bool,
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
            vec![0; capacity]
        };
        Self {
            data,
            writable: true,
        }
    }

    pub fn from_bytes(bytes: &[Byte]) -> Self {
        let mut memory = Self::with_capacity(bytes.len());
        memory.data = bytes.to_vec();
        memory
    }

    pub fn rom(capacity: usize) -> Self {
        let mut rom = Self::with_capacity(capacity);
        rom.writable = false;
        rom
    }
    pub fn rom_from_bytes(bytes: &[Byte]) -> Self {
        let mut rom = Self::rom(bytes.len());
        rom.data = bytes.to_vec();
        rom
    }

    pub fn ram(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }
    pub fn ram_from_bytes(bytes: &[Byte]) -> Self {
        let mut ram = Self::ram(bytes.len());
        ram.data = bytes.to_vec();
        ram
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
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
    #[inline]
    fn readw(&mut self, addr: Word) -> Byte {
        self.peekw(addr)
    }
    #[inline]
    fn peek(&self, addr: Addr) -> Byte {
        self.peekw(addr as Word)
    }
    #[inline]
    fn peekw(&self, addr: Word) -> Byte {
        debug_assert!(addr < self.data.len(), "address outside memory range");
        self.data[addr]
    }
}

impl MemWrite for Memory {
    #[inline]
    fn write(&mut self, addr: Addr, val: Byte) {
        self.writew(addr as Word, val);
    }
    #[inline]
    fn writew(&mut self, addr: Word, val: Byte) {
        if self.writable {
            debug_assert!(addr < self.data.len(), "address outside memory range");
            self.data[addr] = val;
        }
    }
}

impl Bankable for Memory {
    type Item = Self;

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

#[derive(Clone)]
struct Bank {
    start: usize,
    end: usize,
    address: usize,
}

impl Bank {
    #[inline]
    fn new(start: Addr, end: Addr) -> Self {
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
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(
            f,
            "Bank {{ start: 0x{:04X}, end: 0x{:04X}, address: 0x{:04X} }}",
            self.start, self.end, self.address,
        )
    }
}

#[derive(Debug, Clone)]
pub struct BankedMemory {
    banks: Vec<Bank>,
    window: usize,
    bank_shift: usize,
    bank_count: usize,
    memory: Memory,
}

impl BankedMemory {
    #[inline]
    pub fn new(window: usize) -> Self {
        Self::ram(0x2000, window)
    }

    #[inline]
    pub fn ram(capacity: usize, window: usize) -> Self {
        let memory = Memory::ram(capacity);
        Self {
            banks: Vec::new(),
            window,
            bank_shift: Self::bank_shift(window),
            bank_count: memory.len() / window,
            memory,
        }
    }

    #[inline]
    pub fn from(memory: Memory, window: usize) -> Self {
        Self {
            banks: Vec::new(),
            window,
            bank_shift: Self::bank_shift(window),
            bank_count: memory.len() / window,
            memory,
        }
    }

    #[inline]
    pub fn add_bank(&mut self, start: Addr, end: Addr) {
        self.banks.push(Bank::new(start, end));
        self.update_banks();
    }

    #[inline]
    pub fn add_bank_range(&mut self, start: Addr, end: Addr) {
        for start in (start..end).step_by(self.window) {
            let end = start + (self.window as Addr).saturating_sub(1);
            self.banks.push(Bank::new(start, end));
        }
        self.update_banks();
    }

    pub fn set_bank(&mut self, bank_start: Addr, new_bank: usize) {
        let bank = self.get_bank(bank_start);
        debug!(
            "set_bank: 0x{:04X} -> {} / {}",
            bank_start,
            new_bank,
            self.last_bank()
        );
        debug_assert!(bank < self.banks.len(), "bank is outside bankable range");
        debug_assert!(
            new_bank <= self.last_bank(),
            "new_bank is outside bankable range"
        );
        self.banks[bank].address = new_bank * self.window;
    }

    #[inline]
    pub fn last_bank(&self) -> usize {
        self.bank_count.saturating_sub(1)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.memory.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.memory.is_empty()
    }
    #[inline]
    pub fn write_protect(&mut self, protect: bool) {
        self.memory.write_protect(protect);
    }

    fn update_banks(&mut self) {
        self.banks.sort_by(|a, b| a.start.cmp(&b.start));
        let mut address = 0x0000;
        for bank in self.banks.iter_mut() {
            bank.address = address;
            address += bank.end - bank.start + 1;
        }
        debug!("update_banks: {:?}", self.banks);
    }

    fn get_bank(&self, addr: Addr) -> usize {
        let addr = addr as usize;
        let base_addr = if let Some(bank) = self.banks.first() {
            bank.start
        } else {
            0x0000
        };
        debug_assert!(addr >= base_addr, "address is less than base address");
        debug!(
            "get_bank: (0x{:04X} - 0x{:04X}) >> {}",
            addr, base_addr, self.bank_shift
        );
        (addr - base_addr) >> self.bank_shift
    }

    fn translate_addr(&self, addr: Addr) -> usize {
        let bank = self.get_bank(addr);
        debug_assert!(bank < self.banks.len(), "bank is outside bankable range");
        let bank = &self.banks[bank];
        debug!(
            "translate_addr: 0x{:04X} -> 0x{:04X}",
            addr,
            bank.address + (addr as usize - bank.start)
        );
        bank.address + (addr as usize - bank.start)
    }

    #[inline]
    fn bank_shift(mut window: usize) -> usize {
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
    #[inline]
    fn readw(&mut self, addr: Word) -> Byte {
        self.peekw(addr)
    }
    #[inline]
    fn peek(&self, addr: Addr) -> Byte {
        let addr = self.translate_addr(addr);
        debug!("peek: 0x{:04X} (0x{:04X})", addr, self.memory.len());
        self.peekw(addr)
    }
    #[inline]
    fn peekw(&self, addr: Word) -> Byte {
        self.memory.peekw(addr)
    }
}

impl MemWrite for BankedMemory {
    #[inline]
    fn write(&mut self, addr: Addr, val: Byte) {
        let addr = self.translate_addr(addr);
        self.writew(addr, val);
    }
    #[inline]
    fn writew(&mut self, addr: Word, val: Byte) {
        self.memory.writew(addr, val);
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

#[derive(Default, Clone)]
pub struct Banks<T>
where
    T: MemRead + MemWrite + Bankable,
{
    banks: Vec<T::Item>,
    size: usize,
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
        let mut banks: Vec<T::Item> = Vec::with_capacity(data.len());
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
    type Target = Vec<T::Item>;
    fn deref(&self) -> &Vec<T::Item> {
        &self.banks
    }
}

impl<T> DerefMut for Banks<T>
where
    T: MemRead + MemWrite + Bankable,
{
    fn deref_mut(&mut self) -> &mut Vec<T::Item> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_bank_range_test() {
        let mut memory = BankedMemory::new(0x2000);
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

        let mut memory = BankedMemory::new(0x2000);
        memory.add_bank_range(0x8000, 0xBFFF);
        assert_eq!(memory.get_bank(0x8000), 0);
        assert_eq!(memory.get_bank(0x9FFF), 0);
        assert_eq!(memory.get_bank(0xA000), 1);
        assert_eq!(memory.get_bank(0xBFFF), 1);

        memory.add_bank(0x6000, 0x7FFF);
        assert_eq!(memory.get_bank(0x6000), 0);
        assert_eq!(memory.get_bank(0x8000), 1);

        let mut memory = BankedMemory::new(0x0400);
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
        let mut memory = BankedMemory::new(0x4000);
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
        let rom = Memory::ram(size);
        let mut memory = BankedMemory::from(rom, 0x2000);

        assert!(!memory.is_empty(), "memory non-empty");
        assert_eq!(memory.len(), size, "memory size");

        memory.add_bank_range(0x8000, 0xFFFF);
        memory.memory.write(0x0000, 1);
        memory.memory.write(0x0000 + 1, 2);
        memory.memory.write(0x2000, 3);
        memory.memory.write(0x2000 + 1, 4);
        memory.memory.write(0x4000, 5);
        memory.memory.write(0x4000 + 1, 6);
        memory.memory.write(0x6000, 7);
        memory.memory.write(0x6000 + 1, 8);

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
        let rom = Memory::ram(size);
        let mut memory = BankedMemory::from(rom, 0x2000);

        assert!(!memory.is_empty(), "memory non-empty");
        assert_eq!(memory.len(), size, "memory size");

        memory.add_bank_range(0x8000, 0xFFFF);
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
        pretty_env_logger::init_timed();

        let size = 128 * 1024;
        let rom = Memory::ram(size);
        let mut memory = BankedMemory::from(rom, 0x2000);

        assert!(!memory.is_empty(), "memory non-empty");
        assert_eq!(memory.len(), size, "memory size");
        assert_eq!(memory.last_bank(), 15, "bank count");

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
    }
}
