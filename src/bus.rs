use crate::{
    apu::Apu,
    common::Powered,
    input::Input,
    mapper::MapperRef,
    memory::{Memory, Ram},
    nes_err,
    ppu::Ppu,
    serialization::Savable,
    NesResult,
};
use std::{
    collections::HashMap,
    fmt,
    io::{Read, Write},
};

const WRAM_SIZE: usize = 2 * 1024; // 2K NES Work Ram

/// NES Bus
///
/// [http://wiki.nesdev.com/w/index.php/CPU_memory_map]()
pub struct Bus {
    pub ppu: Ppu,
    pub apu: Apu,
    pub mapper: Option<MapperRef>,
    pub input: Input,
    open_bus: u8,
    wram: Ram,
    genie_codes: HashMap<u16, GenieCode>,
    genie_map: HashMap<char, u8>,
}

/// Game Genie Code
struct GenieCode {
    code: String,
    data: u8,
    compare: Option<u8>,
}

impl Bus {
    pub fn new() -> Self {
        let mut genie_map = HashMap::new();
        genie_map.insert('A', 0x0);
        genie_map.insert('P', 0x1);
        genie_map.insert('Z', 0x2);
        genie_map.insert('L', 0x3);
        genie_map.insert('G', 0x4);
        genie_map.insert('I', 0x5);
        genie_map.insert('T', 0x6);
        genie_map.insert('Y', 0x7);
        genie_map.insert('E', 0x8);
        genie_map.insert('O', 0x9);
        genie_map.insert('X', 0xA);
        genie_map.insert('U', 0xB);
        genie_map.insert('K', 0xC);
        genie_map.insert('S', 0xD);
        genie_map.insert('V', 0xE);
        genie_map.insert('N', 0xF);

        Self {
            ppu: Ppu::new(),
            apu: Apu::new(),
            input: Input::new(),
            mapper: None,
            open_bus: 0u8,
            wram: Ram::init(WRAM_SIZE),
            genie_codes: HashMap::new(),
            genie_map,
        }
    }

    pub fn load_mapper(&mut self, mapper: MapperRef) {
        self.ppu.load_mapper(mapper.clone());
        self.apu.load_mapper(mapper.clone());
        self.mapper = Some(mapper);
    }

    pub fn add_genie_code(&mut self, code: &str) -> NesResult<()> {
        if code.len() != 6 && code.len() != 8 {
            return nes_err!("invalid game genie code length");
        }
        let mut hex: Vec<u8> = Vec::with_capacity(code.len());
        for s in code.chars() {
            if let Some(h) = self.genie_map.get(&s) {
                hex.push(*h);
            } else {
                return nes_err!("invalid game genie code");
            }
        }
        let addr = 0x8000
            + (((u16::from(hex[3]) & 7) << 12)
                | ((u16::from(hex[5]) & 7) << 8)
                | ((u16::from(hex[4]) & 8) << 8)
                | ((u16::from(hex[2]) & 7) << 4)
                | ((u16::from(hex[1]) & 8) << 4)
                | (u16::from(hex[4]) & 7)
                | (u16::from(hex[3]) & 8));
        let data = if hex.len() == 6 {
            ((hex[1] & 7) << 4) | ((hex[0] & 8) << 4) | (hex[0] & 7) | (hex[5] & 8)
        } else {
            ((hex[1] & 7) << 4) | ((hex[0] & 8) << 4) | (hex[0] & 7) | (hex[7] & 8)
        };
        let compare = if hex.len() == 8 {
            Some(((hex[7] & 7) << 4) | ((hex[6] & 8) << 4) | (hex[6] & 7) | (hex[5] & 8))
        } else {
            None
        };
        self.genie_codes.insert(
            addr,
            GenieCode {
                code: code.to_string(),
                data,
                compare,
            },
        );
        Ok(())
    }

    pub fn remove_genie_code(&mut self, code: &str) {
        self.genie_codes.retain(|_, gc| gc.code != code);
    }

    fn genie_code(&self, addr: u16) -> Option<&GenieCode> {
        if self.genie_codes.is_empty() {
            None
        } else {
            self.genie_codes.get(&addr)
        }
    }
}

impl Memory for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        // Order of frequently accessed
        let val = match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram.read(addr & 0x07FF), // 0x0800..=0x1FFFF are mirrored
            0x4020..=0xFFFF => {
                if let Some(mapper) = &self.mapper {
                    if let Some(gc) = self.genie_code(addr) {
                        if let Some(compare) = gc.compare {
                            let val = mapper.borrow_mut().read(addr);
                            if val == compare {
                                gc.data
                            } else {
                                val
                            }
                        } else {
                            gc.data
                        }
                    } else {
                        mapper.borrow_mut().read(addr)
                    }
                } else {
                    self.open_bus
                }
            }
            0x4000..=0x4013 | 0x4015 => self.apu.read(addr),
            0x4016..=0x4017 => self.input.read(addr),
            0x2000..=0x3FFF => self.ppu.read(addr & 0x2007), // 0x2008..=0x3FFF are mirrored
            0x4018..=0x401F => self.open_bus,                // APU/IO Test Mode
            0x4014 => self.open_bus,
        };
        self.open_bus = val;
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        // Order of frequently accessed
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram.peek(addr & 0x07FF), // 0x0800..=0x1FFFF are mirrored
            0x4020..=0xFFFF => {
                if let Some(mapper) = &self.mapper {
                    if let Some(gc) = self.genie_code(addr) {
                        if let Some(compare) = gc.compare {
                            let val = mapper.borrow_mut().read(addr);
                            if val == compare {
                                gc.data
                            } else {
                                val
                            }
                        } else {
                            gc.data
                        }
                    } else {
                        mapper.borrow_mut().read(addr)
                    }
                } else {
                    self.open_bus
                }
            }
            0x4000..=0x4013 | 0x4015 => self.apu.peek(addr),
            0x4016..=0x4017 => self.input.peek(addr),
            0x2000..=0x3FFF => self.ppu.peek(addr & 0x2007), // 0x2008..=0x3FFF are mirrored
            0x4018..=0x401F => self.open_bus,                // APU/IO Test Mode
            0x4014 => self.open_bus,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        // Order of frequently accessed
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram.write(addr & 0x07FF, val), // 0x8000..=0x1FFFF are mirrored
            0x4020..=0xFFFF => {
                if let Some(mapper) = &self.mapper {
                    mapper.borrow_mut().write(addr, val);
                }
            }
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, val),
            0x4016 => self.input.write(addr, val),
            0x2000..=0x3FFF => {
                self.ppu.write(addr & 0x2007, val); // 0x2008..=0x3FFF are mirrored
                                                    // Some mappers monitor internal PPU state from the Bus
                if addr <= 0x2006 {
                    if let Some(mapper) = &self.mapper {
                        mapper.borrow_mut().bus_write(addr, val);
                    }
                }
            }
            0x4018..=0x401F => (), // APU/IO Test Mode
            0x4014 => (),          // Handled inside the CPU
        }
    }
}

impl Powered for Bus {
    fn reset(&mut self) {
        self.apu.reset();
        self.ppu.reset();
        if let Some(mapper) = &self.mapper {
            mapper.borrow_mut().reset();
        }
    }
    fn power_cycle(&mut self) {
        self.apu.power_cycle();
        self.ppu.power_cycle();
        if let Some(mapper) = &self.mapper {
            mapper.borrow_mut().power_cycle();
        }
    }
}

impl Savable for Bus {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.wram.save(fh)?;
        self.open_bus.save(fh)?;
        self.ppu.save(fh)?;
        self.apu.save(fh)?;
        if let Some(mapper) = &self.mapper {
            mapper.borrow().save(fh)?;
        }
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.wram.load(fh)?;
        self.open_bus.load(fh)?;
        self.ppu.load(fh)?;
        self.apu.load(fh)?;
        if let Some(mapper) = &self.mapper {
            mapper.borrow_mut().load(fh)?;
        }
        Ok(())
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Bus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Bus {{ }}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_bus() {
        use crate::input::Input;
        use crate::mapper;
        use std::cell::RefCell;
        use std::path::PathBuf;
        use std::rc::Rc;

        let test_rom = "tests/cpu/nestest.nes";
        let rom = PathBuf::from(test_rom);
        let mapper = mapper::load_rom(rom).expect("loaded mapper");
        let input = Rc::new(RefCell::new(Input::new()));
        let mut mem = Bus::init(input);
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
