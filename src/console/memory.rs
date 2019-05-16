use crate::console::apu::Apu;
use crate::console::cartridge::{Board, BoardRef};
use crate::console::input::Input;
use crate::console::ppu::Ppu;
use crate::console::InputRef;
use std::fmt;

pub const KILOBYTE: usize = 0x0400; // 1024 bytes
const DEFAULT_RAM_SIZE: usize = 2 * KILOBYTE;

pub type Addr = u16;
pub type Word = u16;
pub type Byte = u8;

/// Memory Trait

pub trait Memory: fmt::Debug {
    fn readb(&mut self, addr: Addr) -> Byte;
    fn writeb(&mut self, addr: Addr, val: Byte);

    fn readw(&mut self, addr: Addr) -> Word {
        let lo = Addr::from(self.readb(addr));
        let hi = Addr::from(self.readb(addr.wrapping_add(1)));
        lo | hi << 8
    }

    fn writew(&mut self, addr: Addr, val: Word) {
        self.writeb(addr, (val & 0xFF) as Byte);
        self.writeb(addr.wrapping_add(1), ((val >> 8) & 0xFF) as Byte);
    }

    // Same as readw but wraps around for address 0xFF
    fn readw_zp(&mut self, addr: Byte) -> Word {
        let lo = Addr::from(self.readb(Addr::from(addr)));
        let hi = Addr::from(self.readb(Addr::from(addr.wrapping_add(1))));
        lo | hi << 8
    }

    // Emulates a 6502 bug that caused the low byte to wrap without incrementing the high byte
    // e.g. reading from 0x01FF will read from 0x0100
    fn readw_pagewrap(&mut self, addr: Addr) -> Word {
        let lo = Addr::from(self.readb(addr));
        let addr = (addr & 0xFF00) | Addr::from(addr.wrapping_add(1) as Byte);
        let hi = Addr::from(self.readb(addr));
        lo | hi << 8
    }
}

/// Generic RAM

pub struct Ram {
    bytes: Vec<Byte>,
}

impl Ram {
    pub fn new() -> Self {
        Self {
            bytes: vec![0; DEFAULT_RAM_SIZE],
        }
    }

    pub fn with_capacity(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
        }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }
}

impl Memory for Ram {
    fn readb(&mut self, addr: Addr) -> Byte {
        let len = self.bytes.len();
        assert!(len != 0, "Ram length is 0! {:?}", self);
        assert!(
            (addr as usize) < len,
            "Ram read 0x{:04X} within bounds {:?}",
            addr,
            self
        );
        self.bytes[addr as usize]
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        let len = self.bytes.len();
        assert!(len != 0, "Ram length is 0! {:?}", self);
        assert!(
            (addr as usize) < len,
            "Ram write 0x{:04X} within bounds {:?}",
            addr,
            self
        );
        self.bytes[addr as usize] = val;
    }
}

impl fmt::Debug for Ram {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ram {{ bytes: {}KB }}", self.bytes.len() / KILOBYTE)
    }
}

/// Generic ROM
///
pub struct Rom {
    pub bytes: Vec<Byte>,
}

impl Rom {
    pub fn new() -> Self {
        Self {
            bytes: vec![0; DEFAULT_RAM_SIZE],
        }
    }

    pub fn with_capacity(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
        }
    }

    pub fn with_bytes(bytes: Vec<Byte>) -> Self {
        Self { bytes }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }
}

impl Memory for Rom {
    fn readb(&mut self, addr: Addr) -> Byte {
        let len = self.bytes.len();
        self.bytes[addr as usize]
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        eprintln!("writing to read-only rom");
        // ROM is read-only
    }
}

impl fmt::Debug for Rom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Rom {{ bytes: {}KB }}", self.bytes.len() / KILOBYTE)
    }
}

/// CPU Memory Map
///
/// http://wiki.nesdev.com/w/index.php/CPU_memory_map
pub struct CpuMemMap {
    ram: Ram,
    pub ppu: Ppu,
    pub apu: Apu,
    pub board: BoardRef,
    pub input: InputRef,
}

impl CpuMemMap {
    pub fn init(board: BoardRef, input: InputRef) -> Self {
        Self {
            ram: Ram::new(),
            ppu: Ppu::init(board.clone()),
            apu: Apu::new(),
            input,
            board,
        }
    }
}

impl Memory for CpuMemMap {
    fn readb(&mut self, addr: Addr) -> Byte {
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.ram.readb(addr % 0x0800), // 0x8000..=0x1FFFF are mirrored
            0x2000..=0x3FFF => self.ppu.readb(0x2000 + addr % 8), // 0x2008..=0x3FFF are mirrored
            0x4000..=0x4015 => self.apu.readb(addr),
            0x4016..=0x4017 => {
                let mut input = self.input.borrow_mut();
                input.readb(addr)
            }
            0x4018..=0x401F => 0, // APU/IO Test Mode
            0x4020..=0xFFFF => {
                let mut board = self.board.borrow_mut();
                board.readb(addr)
            }
            _ => {
                eprintln!("unhandled CpuMemMap readb at 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.ram.writeb(addr % 0x0800, val), // 0x8000..=0x1FFFF are mirrored
            0x2000..=0x3FFF => self.ppu.writeb(0x2000 + addr % 8, val), // 0x2008..=0x3FFF are mirrored
            0x4000..=0x4015 | 0x4017 => self.apu.writeb(addr, val),
            0x4016 => {
                let mut input = self.input.borrow_mut();
                input.writeb(addr, val);
            }
            0x4018..=0x401F => (), // APU/IO Test Mode
            0x4020..=0xFFFF => {
                let mut board = self.board.borrow_mut();
                board.writeb(addr, val);
            }
            _ => {
                eprintln!(
                    "unhandled CpuMemMap writeb at 0x{:04X} - val: 0x{:02x}",
                    addr, val
                );
            }
        }
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
    fn test_readw_pagewrap() {
        let mut memory = Ram::new();
        memory.writeb(0x0100, 0xDE);
        memory.writeb(0x01FF, 0xAD);
        memory.writeb(0x0200, 0x11);
        let val = memory.readw_pagewrap(0x01FF);
        assert_eq!(
            val, 0xDEAD,
            "readw_pagewrap 0x{:04X} == 0x{:04X}",
            val, 0xDEAD
        );
    }

    #[test]
    fn test_readw_zp() {
        let mut memory = Ram::new();
        memory.writeb(0x00, 0xDE);
        memory.writeb(0xFF, 0xAD);
        let val = memory.readw_zp(0xFF);
        assert_eq!(val, 0xDEAD, "readw_zp 0x{:04X} == 0x{:04X}", val, 0xDEAD);
    }

    #[test]
    fn test_cpu_memory() {
        use crate::console::cartridge::Cartridge;
        use crate::console::input::Input;
        use std::cell::RefCell;
        use std::path::PathBuf;
        use std::rc::Rc;

        let test_rom = "tests/cpu/nestest.nes";
        let rom = &PathBuf::from(test_rom);
        let board = Cartridge::new(rom)
            .expect("cartridge")
            .load_board()
            .expect("loaded board");
        let input = Rc::new(RefCell::new(Input::new()));
        let mut mem = CpuMemMap::init(board, input);
        mem.writeb(0x0005, 0x0015);
        mem.writeb(0x0015, 0x0050);
        mem.writeb(0x0016, 0x0025);

        assert_eq!(mem.readb(0x0008), 0x00, "read uninitialized byte: 0x00");
        assert_eq!(mem.readw(0x0008), 0x0000, "read uninitialized word: 0x0000");
        assert_eq!(
            mem.readb(0x0005),
            0x15,
            "read initialized byte: 0x{:02X}",
            0x15
        );
        assert_eq!(
            mem.readw(0x0015),
            0x2550,
            "read initialized word: 0x{:04X}",
            0x2550
        );
        assert_eq!(
            mem.readb(0x0808),
            0x00,
            "read uninitialized mirror1 byte: 0x00"
        );
        assert_eq!(
            mem.readw(0x0808),
            0x0000,
            "read uninitialized mirror1 word: 0x0000"
        );
        assert_eq!(
            mem.readb(0x0805),
            0x15,
            "read initialized mirror1 byte: 0x{:02X}",
            0x15,
        );
        assert_eq!(
            mem.readw(0x0815),
            0x2550,
            "read initialized mirror1 word: 0x{:04X}",
            0x2550,
        );
        assert_eq!(
            mem.readb(0x1008),
            0x00,
            "read uninitialized mirror2 byte: 0x00"
        );
        assert_eq!(
            mem.readw(0x1008),
            0x0000,
            "read uninitialized mirror2 word: 0x0000"
        );
        assert_eq!(
            mem.readb(0x1005),
            0x15,
            "read initialized mirror2 byte: 0x{:02X}",
            0x15,
        );
        assert_eq!(
            mem.readw(0x1015),
            0x2550,
            "read initialized mirror2 word: 0x{:04X}",
            0x2550,
        );
        // The following are test mode addresses, Not mapped
        assert_eq!(mem.readb(0x0418), 0x00, "read unmapped byte: 0x00");
        assert_eq!(mem.readb(0x0418), 0x00, "write unmapped byte: 0x00");
        assert_eq!(mem.readw(0x0418), 0x0000, "read unmapped word: 0x0000");
        assert_eq!(mem.readw(0x0418), 0x0000, "read unmapped word: 0x0000");
    }
}
