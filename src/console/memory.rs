use crate::console::cartridge::Board;
use crate::console::ppu::Ppu;
use std::fmt;
use std::sync::{Arc, Mutex};

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
        self.bytes[addr as usize & (len - 1)]
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
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
        self.bytes[addr as usize & (len - 1)]
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        panic!(
            "rom: attempt to write read-only memory 0x{:04X} - value: 0x{:04X}",
            addr, val
        );
    }
}

impl fmt::Debug for Rom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ram {{ bytes: {}KB }}", self.bytes.len() / KILOBYTE)
    }
}

/// CPU Memory Map
///
/// http://wiki.nesdev.com/w/index.php/CPU_memory_map
#[derive(Debug)]
pub struct CpuMemMap {
    ram: Ram,
    pub ppu: Ppu,
    // apu: Apu,
    // input: Input,
    board: Option<Arc<Mutex<Board>>>,
}

impl CpuMemMap {
    pub fn init() -> Self {
        Self {
            ram: Ram::new(),
            ppu: Ppu::new(),
            // apu: Apu::new(),
            // input: Input::new(),
            board: None,
        }
    }

    pub fn set_board(&mut self, board: Arc<Mutex<Board>>) {
        self.ppu.set_board(board.clone());
        self.board = Some(board);
    }
}

impl Memory for CpuMemMap {
    fn readb(&mut self, addr: Addr) -> Byte {
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.ram.readb(addr), // 0x8000..=0x1FFFF are mirrored
            // 0x2000..=0x3FFF => self.ppu.readb(addr & 0x2007), // 0x2008..=0x3FFF are mirrored
            // 0x4000..=0x4015 => self.apu.readb(addr),
            // 0x4016..=0x4017 => self.input.readb(addr),
            // 0x4018..=0x401F => 0, // APU/IO Test Mode
            0x4020..=0xFFFF => {
                if let Some(b) = &self.board {
                    let mut board = b.lock().unwrap();
                    board.readb(addr)
                } else {
                    0
                }
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
            0x0000..=0x1FFF => self.ram.writeb(addr & 0x07FF, val), // 0x8000..=0x1FFFF are mirrored
            // 0x2000..=0x3FFF => self.ppu.writeb(addr & 0x2007, val), // 0x2008..=0x3FFF are mirrored
            // 0x4000..=0x4015 | 0x4017 => self.apu.writeb(addr, val),
            // 0x4016 => self.input.writeb(addr, val),
            // 0x4018..=0x401F => 0, // APU/IO Test Mode
            0x4020..=0xFFFF => {
                if let Some(b) = &self.board {
                    let mut board = b.lock().unwrap();
                    board.writeb(addr, val);
                } else {
                    eprintln!(
                        "uninitialized board at CpuMemMap writeb at 0x{:04X} - val: 0x{:02x}",
                        addr, val
                    );
                }
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
        let mut mem = CpuMemMap::init();
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
