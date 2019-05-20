use crate::console::apu::Apu;
use crate::console::ppu::Ppu;
use crate::input::InputRef;
use crate::mapper::MapperRef;
use std::fmt;

const WRAM_SIZE: usize = 2 * 1024;

/// Memory Trait

pub trait Memory: fmt::Debug {
    fn readb(&mut self, addr: u16) -> u8;
    fn writeb(&mut self, addr: u16, val: u8);
    fn readw(&mut self, addr: u16) -> u16 {
        let lo = u16::from(self.readb(addr));
        let hi = u16::from(self.readb(addr.wrapping_add(1)));
        lo | hi << 8
    }
    fn writew(&mut self, addr: u16, val: u16) {
        self.writeb(addr, (val & 0xFF) as u8);
        self.writeb(addr.wrapping_add(1), ((val >> 8) & 0xFF) as u8);
    }
    // Same as readw but wraps around for address 0xFF
    fn readw_zp(&mut self, addr: u8) -> u16 {
        let lo = u16::from(self.readb(u16::from(addr)));
        let hi = u16::from(self.readb(u16::from(addr.wrapping_add(1))));
        lo | hi << 8
    }
    // Emulates a 6502 bug that caused the low byte to wrap without incrementing the high byte
    // e.g. reading from 0x01FF will read from 0x0100
    fn readw_pagewrap(&mut self, addr: u16) -> u16 {
        let lo = u16::from(self.readb(addr));
        let addr = (addr & 0xFF00) | u16::from(addr.wrapping_add(1) as u8);
        let hi = u16::from(self.readb(addr));
        lo | hi << 8
    }
}

/// CPU Memory Map
///
/// http://wiki.nesdev.com/w/index.php/CPU_memory_map
pub struct CpuMemMap {
    wram: [u8; WRAM_SIZE],
    pub ppu: Ppu,
    pub apu: Apu,
    pub mapper: MapperRef,
    pub input: InputRef,
}

impl CpuMemMap {
    pub fn init(mapper: MapperRef, input: InputRef) -> Self {
        Self {
            wram: [0; WRAM_SIZE],
            ppu: Ppu::init(mapper.clone()),
            apu: Apu::new(),
            input,
            mapper,
        }
    }
}

impl Memory for CpuMemMap {
    fn readb(&mut self, addr: u16) -> u8 {
        // Order of frequently accessed
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram[(addr & 0x07FF) as usize], // 0x0800..=0x1FFFF are mirrored
            0x4020..=0xFFFF => {
                let mut mapper = self.mapper.borrow_mut();
                mapper.readb(addr)
            }
            0x4000..=0x4015 => self.apu.readb(addr),
            0x4016..=0x4017 => {
                let mut input = self.input.borrow_mut();
                input.readb(addr)
            }
            0x2000..=0x3FFF => self.ppu.readb(addr & 0x2007), // 0x2008..=0x3FFF are mirrored
            0x4018..=0x401F => 0,                             // APU/IO Test Mode
            _ => {
                eprintln!("unhandled CpuMemMap readb at 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        // Order of frequently accessed
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram[(addr & 0x07FF) as usize] = val, // 0x8000..=0x1FFFF are mirrored
            0x4020..=0xFFFF => {
                let mut mapper = self.mapper.borrow_mut();
                mapper.writeb(addr, val);
            }
            0x4000..=0x4015 | 0x4017 => self.apu.writeb(addr, val),
            0x4016 => {
                let mut input = self.input.borrow_mut();
                input.writeb(addr, val);
            }
            0x2000..=0x3FFF => self.ppu.writeb(addr & 0x2007, val), // 0x2008..=0x3FFF are mirrored
            0x4018..=0x401F => (),                                  // APU/IO Test Mode
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

    // #[test]
    // fn test_readw_pagewrap() {
    //     let mut memory = Ram::new();
    //     memory.writeb(0x0100, 0xDE);
    //     memory.writeb(0x01FF, 0xAD);
    //     memory.writeb(0x0200, 0x11);
    //     let val = memory.readw_pagewrap(0x01FF);
    //     assert_eq!(
    //         val, 0xDEAD,
    //         "readw_pagewrap 0x{:04X} == 0x{:04X}",
    //         val, 0xDEAD
    //     );
    // }

    // #[test]
    // fn test_readw_zp() {
    //     let mut memory = Ram::new();
    //     memory.writeb(0x00, 0xDE);
    //     memory.writeb(0xFF, 0xAD);
    //     let val = memory.readw_zp(0xFF);
    //     assert_eq!(val, 0xDEAD, "readw_zp 0x{:04X} == 0x{:04X}", val, 0xDEAD);
    // }

    #[test]
    fn test_cpu_memory() {
        use crate::console::cartridge::Cartridge;
        use crate::console::input::Input;
        use std::cell::RefCell;
        use std::path::PathBuf;
        use std::rc::Rc;

        let test_rom = "tests/cpu/nestest.nes";
        let rom = &PathBuf::from(test_rom);
        let mapper = mapper::load_rom(rom).expect("loaded mapper");
        let input = Rc::new(RefCell::new(Input::new()));
        let mut mem = CpuMemMap::init(mapper, input);
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
