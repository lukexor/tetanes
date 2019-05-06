use std::fmt;

pub type Addr = u16;
pub type Word = u16;
pub type Byte = u8;

/// Memory Trait

pub trait Memory: fmt::Debug {
    fn readb(&self, addr: Addr) -> Byte;
    fn writeb(&mut self, addr: Addr, val: Byte);
    fn readw(&self, addr: Addr) -> Word {
        let lo = u16::from(self.readb(addr));
        let hi = u16::from(self.readb(addr.wrapping_add(1)));
        hi << 8 | lo
    }
}

/// Generic RAM

pub struct Ram {
    bytes: Vec<Byte>,
}

impl Ram {
    pub fn new(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
        }
    }
}

impl Memory for Ram {
    fn readb(&self, addr: Addr) -> Byte {
        let len = self.bytes.len();
        self.bytes[addr as usize % len]
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        let len = self.bytes.len();
        self.bytes[addr as usize % len] = val;
    }
}

impl fmt::Debug for Ram {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ram {{ bytes: {}KB }}", self.bytes.len() / 0x400)
    }
}

/// Generic Memory Map

pub struct MemoryMap {
    // (begin, end, memory_index, is_mirror)
    addresses: Vec<(Addr, Addr, usize, bool)>,
    memory: Vec<Box<Memory>>,
}

impl MemoryMap {
    pub fn new(size: usize) -> Self {
        Self {
            addresses: Vec::with_capacity(size),
            memory: Vec::with_capacity(size),
        }
    }

    pub fn map<M: Memory + 'static>(&mut self, begin: Addr, end: Addr, memory: Box<M>) {
        let _ = self.add_map(begin, end, memory);
    }

    pub fn map_mirrored<M: Memory + 'static>(
        &mut self,
        begin: Addr,
        end: Addr,
        mirror_begin: Addr,
        mirror_end: Addr,
        memory: Box<M>,
    ) {
        let mem_index = self.add_map(begin, end, memory);
        self.addresses
            .push((mirror_begin, mirror_end, mem_index, true));
    }

    fn add_map<M: Memory + 'static>(&mut self, begin: Addr, end: Addr, memory: Box<M>) -> usize {
        if let Some(i) = self.get_memory_idx(begin) {
            panic!(
                "address map conflict: 0x{:04X}..=0x{:04X} with index {}\n{:?}",
                begin, end, i, self
            );
        }

        let memory_index = self.memory.len();
        self.addresses.push((begin, end, memory_index, false));
        self.memory.push(memory);
        memory_index
    }

    fn get_memory_idx(&self, addr: Addr) -> Option<usize> {
        // If address matches exactly, return its index,
        // otherwise it returns the index+1
        let search = self.addresses.binary_search_by(|m| m.0.cmp(&addr));
        match search {
            Ok(i) => Some(self.addresses[i].2),
            // Didn't find a match, check the upper bound on the previous index
            // to ensure it still fits the mapped range
            Err(i) => {
                if let Some(a) = self.addresses.get(i.saturating_sub(1)) {
                    if addr <= a.1 {
                        return Some(a.2);
                    }
                }
                None
            }
        }
    }
}

impl Memory for MemoryMap {
    fn readb(&self, addr: Addr) -> Byte {
        match self.get_memory_idx(addr) {
            Some(i) => self.memory[i].readb(addr),
            None => {
                eprintln!("unhandled memory readb at 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        match self.get_memory_idx(addr) {
            Some(i) => self.memory[i].writeb(addr, val),
            None => eprintln!("unhandled memory writeb 0x{:04X} at 0x{:04X}", val, addr),
        }
    }

    fn readw(&self, addr: Addr) -> Word {
        match self.get_memory_idx(addr) {
            Some(i) => self.memory[i].readw(addr),
            None => {
                eprintln!("unhandled memory readb at 0x{:04X}", addr);
                0
            }
        }
    }
}

impl fmt::Debug for MemoryMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut str = String::from(format!("MemoryMap {{"));
        for (begin, end, index, mirrored) in &self.addresses {
            str.push_str(&format!(
                "\n    0x{:04X}..=0x{:04X} => {:?}",
                begin, end, self.memory[*index]
            ));
            if *mirrored {
                str.push_str(" (Mirrored)");
            }
        }
        write!(f, "{}}}", str)
    }
}

// const MIRROR_LOOKUP: [[u8; 4]; 5] = [
//     [0, 0, 1, 1],
//     [0, 1, 0, 1],
//     [0, 0, 0, 0],
//     [1, 1, 1, 1],
//     [0, 1, 2, 3],
// ];

// pub fn readb(c: &mut Console, addr: u16) -> u8 {
//     let val = match addr {
//         0x0000...0x1FFF => c.ram[(addr % 0x0800) as usize],
//         0x2000...0x3FFF => read_ppu_register(c, 0x2000 + addr % 8),
//         0x4000...0x4013 => c.apu.read_register(addr),
//         0x4014 => read_ppu_register(c, addr),
//         0x4015 => c.apu.read_register(addr),
//         0x4016 => c.controller1.read(),
//         0x4017 => c.controller2.read(),
//         0x4018...0x5FFF => 0, // TODO I/O
//         0x6000..=0xFFFF => {
//             if let Some(mapper) = &c.mapper {
//                 mapper.readb(addr)
//             } else {
//                 0
//             }
//         }
//     };
//     #[cfg(debug_assertions)]
//     {
//         if c.trace > 1 {
//             println!("readb 0x{:04X} from 0x{:04X}", val, addr);
//         }
//     }
//     val
// }

// pub fn writeb(c: &mut Console, addr: u16, val: u8) {
//     #[cfg(debug_assertions)]
//     {
//         if c.trace > 1 {
//             println!("writeb 0x{:04X} to 0x{:04X}", val, addr);
//         }
//     }
//     match addr {
//         0x0000...0x1FFF => c.ram[(addr % 0x8000) as usize] = val,
//         0x2000...0x3FFF => write_ppu_register(c, 0x2000 + addr % 8, val),
//         0x4000...0x4013 => c.apu.write_register(addr, val),
//         0x4014 => write_ppu_register(c, addr, val),
//         0x4015 => c.apu.write_register(addr, val),
//         0x4016 => {
//             c.controller1.write(val);
//             c.controller2.write(val);
//         }
//         0x4017 => c.apu.write_register(addr, val),
//         0x4018...0x5FFF => (), // TODO I/O
//         0x6000..=0xFFFF => {
//             if let Some(mapper) = &mut c.mapper {
//                 mapper.writeb(addr, val);
//             }
//         }
//     }
// }

// pub fn read_ppu_register(c: &mut Console, addr: u16) -> u8 {
//     match addr {
//         0x2002 => {
//             let mut result = c.ppu.register & 0x1F;
//             result |= c.ppu.flag_sprite_overflow << 5;
//             result |= c.ppu.flag_sprite_zero_hit << 6;
//             if c.ppu.nmi_occurred {
//                 result |= 1 << 7;
//             }
//             c.ppu.nmi_occurred = false;
//             c.ppu.nmi_change();
//             c.ppu.w = 0;
//             result
//         }
//         0x2004 => c.ppu.oam_data[c.ppu.oam_address as usize],
//         0x2007 => {
//             let mut val = read_ppu(c, c.ppu.v);
//             if (c.ppu.v % 0x4000) < 0x3F00 {
//                 std::mem::swap(&mut c.ppu.buffered_data, &mut val)
//             } else {
//                 c.ppu.buffered_data = read_ppu(c, c.ppu.v - 0x1000);
//             }
//             if c.ppu.flag_increment {
//                 c.ppu.v = c.ppu.v.wrapping_add(32) & 0x3FFF;
//             } else {
//                 c.ppu.v = c.ppu.v.wrapping_add(1) & 0x3FFF;
//             }
//             val
//         }
//         _ => 0,
//     }
// }

// pub fn write_ppu_register(c: &mut Console, addr: u16, val: u8) {
//     c.ppu.register = val;
//     match addr {
//         0x2000 => c.ppu.write_control(val),
//         0x2001 => c.ppu.write_mask(val),
//         0x2003 => c.ppu.oam_address = val,
//         0x2004 => {
//             // write oam data
//             c.ppu.oam_data[c.ppu.oam_address as usize] = val;
//             c.ppu.oam_address = c.ppu.oam_address.wrapping_add(1);
//         }
//         0x2005 => {
//             // write scroll
//             if c.ppu.w == 0 {
//                 c.ppu.t = (c.ppu.t & 0xFFE0) | (u16::from(val) >> 3);
//                 c.ppu.x = val & 0x07;
//                 c.ppu.w = 1;
//             } else {
//                 c.ppu.t = (c.ppu.t & 0x8FFF) | ((u16::from(val) & 0x07) << 12);
//                 c.ppu.t = (c.ppu.t & 0xFC1F) | ((u16::from(val) & 0xF8) << 2);
//                 c.ppu.w = 0;
//             }
//         }
//         0x2006 => {
//             // write address
//             if c.ppu.w == 0 {
//                 c.ppu.t = (c.ppu.t & 0x80FF) | ((u16::from(val) & 0x3F) << 8);
//                 c.ppu.w = 1;
//             } else {
//                 c.ppu.t = (c.ppu.t & 0xFF00) | u16::from(val);
//                 c.ppu.v = c.ppu.t;
//                 c.ppu.w = 0;
//             }
//         }
//         0x2007 => {
//             // write data
//             write_ppu(c, c.ppu.v, val);
//             if c.ppu.flag_increment {
//                 c.ppu.v = c.ppu.v.wrapping_add(32) & 0x3FFF;
//             } else {
//                 c.ppu.v = c.ppu.v.wrapping_add(1) & 0x3FFF;
//             }
//         }
//         0x4014 => {
//             let mut addr = u16::from(val) << 8;
//             for _ in 0..256 {
//                 c.ppu.oam_data[c.ppu.oam_address as usize] = readb(c, addr);
//                 c.ppu.oam_address = c.ppu.oam_address.wrapping_add(1);
//                 addr += 1;
//             }
//             c.cpu.stall += 513;
//             if c.cpu.cycles % 2 == 1 {
//                 c.cpu.stall += 1;
//             }
//         }
//         _ => panic!("unhandled PPU register write at address 0x{:04X}", addr),
//     }
// }

// pub fn read_ppu(c: &mut Console, mut addr: u16) -> u8 {
//     addr %= 0x4000;
//     match addr {
//         0x0000...0x1FFF => {
//             if let Some(mapper) = &c.mapper {
//                 mapper.readb(addr)
//             } else {
//                 0
//             }
//         }
//         0x2000...0x3EFF => {
//             if let Some(mapper) = &c.mapper {
//                 let addr = mirror_address(mapper.mirror(), addr) % 2048;
//                 c.ppu.name_table_data(addr)
//             } else {
//                 0
//             }
//         }
//         0x3F00...0x4000 => c.ppu.read_palette(addr % 32),
//         _ => panic!("unhandled PPU memory read at addr 0x{:04X}", addr),
//     }
// }

// pub fn write_ppu(c: &mut Console, mut addr: u16, val: u8) {
//     addr %= 0x4000;
//     match addr {
//         0x0000...0x1FFF => {
//             if let Some(mapper) = &mut c.mapper {
//                 mapper.writeb(addr, val);
//             }
//         }
//         0x2000...0x3EFF => {
//             if let Some(mapper) = &c.mapper {
//                 let addr = mirror_address(mapper.mirror(), addr) % 2048;
//                 c.ppu.set_name_table_data(addr, val);
//             }
//         }
//         0x3F00...0x4000 => c.ppu.write_palette(addr % 32, val),
//         _ => panic!("unhandled PPU memory write at addr 0x{:04X}", addr),
//     }
// }

// fn mirror_address(mode: u8, mut addr: u16) -> u16 {
//     addr = (addr - 0x2000) % 0x1000;
//     let table = addr / 0x0400;
//     let offset = addr % 0x0400;
//     0x2000 + u16::from(MIRROR_LOOKUP[mode as usize][table as usize]) * 0x0400 + offset
// }
