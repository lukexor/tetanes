//! Memory Map

use crate::console::apu::Apu;
use crate::console::ppu::Ppu;
use crate::input::InputRef;
use crate::mapper::{self, MapperRef};
use crate::serialization::Savable;
use crate::util::Result;
use rand::Rng;
use std::fmt;
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};

pub const FOUR_SCREEN_RAM_SIZE: usize = 4 * 1024;
pub const PRG_RAM_8K: usize = 8 * 1024;
pub const PRG_RAM_32K: usize = 32 * 1024; // 32KB is safely compatible sans NES 2.0 header
pub const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
pub const PRG_ROM_BANK_SIZE: usize = 16 * 1024;
pub const CHR_RAM_SIZE: usize = 8 * 1024;
pub const CHR_ROM_BANK_SIZE: usize = 8 * 1024;
pub const CHR_BANK_SIZE: usize = 4 * 1024;
pub static mut RANDOMIZE_RAM: bool = false;
const WRAM_SIZE: usize = 2 * 1024;

/// Memory Trait
pub trait Memory {
    fn read(&mut self, addr: u16) -> u8;
    fn peek(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
}

impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "")
    }
}

pub struct Ram(Vec<u8>);

impl Ram {
    pub fn init(mut size: usize) -> Self {
        // Ensure we are 16-bit addressable
        if size >= 0x10000 {
            eprintln!("warning: RAM size of {} is not 16-bit addressable", size);
            size = 0x10000;
        }
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
    fn save(&self, fh: &mut Write) -> Result<()> {
        self.0.save(fh)
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
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

pub struct Rom(Vec<u8>);

impl Rom {
    pub fn init(mut size: usize) -> Self {
        // Ensure we are 16-bit addressable
        if size >= 0x10000 {
            eprintln!("warning: ROM size of {} is not 16-bit addressable", size);
            size = 0x10000;
        }
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
    fn save(&self, fh: &mut Write) -> Result<()> {
        self.0.save(fh)
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
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

pub struct Banks<T>
where
    T: Memory + Bankable,
{
    banks: Vec<T>,
    size: usize,
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
        let mut banks: Vec<T> = Vec::new();
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

/// CPU Memory Map
///
/// [http://wiki.nesdev.com/w/index.php/CPU_memory_map]()
pub struct CpuMemMap {
    pub wram: Ram,
    open_bus: u8,
    pub ppu: Ppu,
    pub apu: Apu,
    pub mapper: MapperRef,
    input: InputRef,
}

impl CpuMemMap {
    pub fn init(input: InputRef) -> Self {
        Self {
            wram: Ram::init(WRAM_SIZE),
            open_bus: 0u8,
            ppu: Ppu::init(mapper::null()),
            apu: Apu::new(),
            input,
            mapper: mapper::null(),
        }
    }

    pub fn load_mapper(&mut self, mapper: MapperRef) {
        self.mapper = mapper.clone();
        self.ppu.load_mapper(mapper);
    }
}

impl Memory for CpuMemMap {
    fn read(&mut self, addr: u16) -> u8 {
        // Order of frequently accessed
        let val = match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram.read(addr & 0x07FF), // 0x0800..=0x1FFFF are mirrored
            0x6000..=0xFFFF => {
                let mut mapper = self.mapper.borrow_mut();
                mapper.read(addr)
            }
            0x4000..=0x4013 | 0x4015 => self.apu.read(addr),
            0x4016..=0x4017 => {
                let mut input = self.input.borrow_mut();
                input.read(addr)
            }
            0x2000..=0x3FFF => self.ppu.read(addr & 0x2007), // 0x2008..=0x3FFF are mirrored
            0x4018..=0x401F => self.open_bus,                // APU/IO Test Mode
            0x4014 => self.open_bus,
            _ => self.open_bus,
        };
        self.open_bus = val;
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        // Order of frequently accessed
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram.peek(addr & 0x07FF), // 0x0800..=0x1FFFF are mirrored
            0x6000..=0xFFFF => {
                let mapper = self.mapper.borrow();
                mapper.peek(addr)
            }
            0x4000..=0x4013 | 0x4015 => self.apu.peek(addr),
            0x4016..=0x4017 => {
                let input = self.input.borrow();
                input.peek(addr)
            }
            0x2000..=0x3FFF => self.ppu.peek(addr & 0x2007), // 0x2008..=0x3FFF are mirrored
            0x4018..=0x401F => self.open_bus,                // APU/IO Test Mode
            0x4014 => self.open_bus,
            _ => self.open_bus,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        // Order of frequently accessed
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram.write(addr & 0x07FF, val), // 0x8000..=0x1FFFF are mirrored
            0x6000..=0xFFFF => {
                let mut mapper = self.mapper.borrow_mut();
                mapper.write(addr, val);
            }
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, val),
            0x4016 => {
                let mut input = self.input.borrow_mut();
                input.write(addr, val);
            }
            0x2000..=0x3FFF => self.ppu.write(addr & 0x2007, val), // 0x2008..=0x3FFF are mirrored
            0x4018..=0x401F => (),                                 // APU/IO Test Mode
            0x4014 => (),                                          // Handled inside the CPU
            _ => (),
        }
    }
}

impl Savable for CpuMemMap {
    fn save(&self, fh: &mut Write) -> Result<()> {
        self.wram.save(fh)?;
        self.open_bus.save(fh)?;
        self.ppu.save(fh)?;
        self.apu.save(fh)?;
        {
            let mapper = self.mapper.borrow();
            mapper.save(fh)
        }
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        self.wram.load(fh)?;
        self.open_bus.load(fh)?;
        self.ppu.load(fh)?;
        self.apu.load(fh)?;
        {
            let mut mapper = self.mapper.borrow_mut();
            mapper.load(fh)?;
        }
        Ok(())
    }
}

impl fmt::Debug for CpuMemMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CpuMemMap {{ }}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mirror_offset() {
        // RAM
        let start = 0x0000;
        let end = 0x07FF;

        let mirror_start = 0x0800;
        let mirror_end = 0x1FFF;

        for addr in mirror_start..=mirror_end {
            let addr = addr & end;
            assert!(addr >= start && addr <= end, "Addr within range");
        }

        // PPU
        let start = 0x2000;
        let end = 0x2007;

        let mirror_start = 0x2008;
        let mirror_end = 0x3FFF;

        for addr in mirror_start..=mirror_end {
            let addr = addr & end;
            assert!(addr >= start && addr <= end, "Addr within range");
        }
    }

    #[test]
    fn test_cpu_memory() {
        use crate::input::Input;
        use crate::mapper;
        use std::cell::RefCell;
        use std::path::PathBuf;
        use std::rc::Rc;

        let test_rom = "tests/cpu/nestest.nes";
        let rom = PathBuf::from(test_rom);
        let mapper = mapper::load_rom(rom).expect("loaded mapper");
        let input = Rc::new(RefCell::new(Input::new()));
        let mut mem = CpuMemMap::init(input);
        mem.load_mapper(mapper);
        mem.write(0x0005, 0x0015);
        mem.write(0x0015, 0x0050);
        mem.write(0x0016, 0x0025);

        assert_eq!(mem.read(0x0008), 0x00, "read uninitialized byte: 0x00");
        assert_eq!(
            mem.read(0x0005),
            0x15,
            "read initialized byte: 0x{:02X}",
            0x15
        );
        assert_eq!(
            mem.read(0x0808),
            0x00,
            "read uninitialized mirror1 byte: 0x00"
        );
        assert_eq!(
            mem.read(0x0805),
            0x15,
            "read initialized mirror1 byte: 0x{:02X}",
            0x15,
        );
        assert_eq!(
            mem.read(0x1008),
            0x00,
            "read uninitialized mirror2 byte: 0x00"
        );
        assert_eq!(
            mem.read(0x1005),
            0x15,
            "read initialized mirror2 byte: 0x{:02X}",
            0x15,
        );
        // The following are test mode addresses, Not mapped
        assert_eq!(mem.read(0x0418), 0x00, "read unmapped byte: 0x00");
        assert_eq!(mem.read(0x0418), 0x00, "write unmapped byte: 0x00");
    }
}
