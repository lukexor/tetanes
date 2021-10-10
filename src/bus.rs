use crate::{
    apu::Apu,
    common::{Addr, Byte, Powered},
    hashmap,
    input::Input,
    mapper::{self, Mapper, MapperType},
    memory::{MemRead, MemWrite, Memory},
    nes_err,
    ppu::Ppu,
    serialization::Savable,
    NesResult,
};
use lazy_static::lazy_static;
use std::{
    collections::HashMap,
    fmt,
    io::{Read, Write},
};

const WRAM_SIZE: usize = 2 * 1024; // 2K NES Work Ram

/// NES Bus
///
/// [http://wiki.nesdev.com/w/index.php/CPU_memory_map]()
#[derive(Clone)]
pub struct Bus {
    pub ppu: Ppu,
    pub apu: Apu,
    pub mapper: Box<MapperType>,
    pub input: Input,
    pub wram: Memory,
    genie_codes: HashMap<Addr, GenieCode>,
    open_bus: Byte,
}

/// Game Genie Code
#[derive(Clone)]
struct GenieCode {
    code: String,
    data: Byte,
    compare: Option<Byte>,
}

lazy_static! {
    static ref GENIE_MAP: HashMap<char, Byte> = {
        // Game genie maps these letters to binary representations as a form of code obfuscation
        hashmap! {
            'A' => 0x0, 'P' => 0x1, 'Z' => 0x2, 'L' => 0x3, 'G' => 0x4, 'I' => 0x5, 'T' => 0x6,
            'Y' => 0x7, 'E' => 0x8, 'O' => 0x9, 'X' => 0xA, 'U' => 0xB, 'K' => 0xC, 'S' => 0xD,
            'V' => 0xE, 'N' => 0xF
        }
    };
}

impl Bus {
    pub fn new(consistent_ram: bool) -> Self {
        let mut bus = Self {
            ppu: Ppu::new(),
            apu: Apu::new(),
            input: Input::new(),
            mapper: Box::new(mapper::null()),
            wram: Memory::ram(WRAM_SIZE, consistent_ram),
            genie_codes: HashMap::new(),
            open_bus: 0,
        };
        bus.ppu.load_mapper(&mut bus.mapper);
        bus.apu.load_mapper(&mut bus.mapper);
        bus
    }

    pub fn load_mapper(&mut self, mapper: MapperType) {
        let mut mapper = Box::new(mapper);
        self.ppu.load_mapper(&mut mapper);
        self.apu.load_mapper(&mut mapper);
        self.mapper = mapper;
    }

    pub fn add_genie_code(&mut self, code: &str) -> NesResult<()> {
        if code.len() != 6 && code.len() != 8 {
            return nes_err!("Invalid Game Genie code: {}", code);
        }
        let mut hex: Vec<Byte> = Vec::with_capacity(code.len());
        for s in code.chars() {
            if let Some(h) = GENIE_MAP.get(&s) {
                hex.push(*h);
            } else {
                return nes_err!("Invalid Game Genie code: {}", code);
            }
        }
        let addr = 0x8000
            + (((Addr::from(hex[3]) & 7) << 12)
                | ((Addr::from(hex[5]) & 7) << 8)
                | ((Addr::from(hex[4]) & 8) << 8)
                | ((Addr::from(hex[2]) & 7) << 4)
                | ((Addr::from(hex[1]) & 8) << 4)
                | (Addr::from(hex[4]) & 7)
                | (Addr::from(hex[3]) & 8));
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

    fn genie_code(&self, addr: Addr) -> Option<GenieCode> {
        if self.genie_codes.is_empty() {
            None
        } else {
            self.genie_codes.get(&addr).cloned()
        }
    }
}

impl MemRead for Bus {
    fn read(&mut self, addr: Addr) -> Byte {
        // Order of frequently accessed
        let val = match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram.read(addr & 0x07FF), // 0x0800..=0x1FFF are mirrored
            0x4020..=0xFFFF => {
                let gc = self.genie_code(addr);
                if let Some(gc) = gc {
                    if let Some(compare) = gc.compare {
                        let val = self.mapper.read(addr);
                        if val == compare {
                            gc.data
                        } else {
                            val
                        }
                    } else {
                        gc.data
                    }
                } else {
                    self.mapper.read(addr)
                }
            }
            0x4000..=0x4013 | 0x4015 => self.apu.read(addr),
            0x4016..=0x4017 => self.input.read(addr),
            0x2000..=0x3FFF => self.ppu.read(addr & 0x2007), // 0x2008..=0x3FFF are mirrored
            0x4018..=0x401F => self.open_bus,                // APU/IO Test Mode
            0x4014 => self.open_bus,
        };
        // Helps to sync open bus behavior
        self.mapper.open_bus(addr, val);
        self.open_bus = val;
        val
    }

    fn peek(&self, addr: Addr) -> Byte {
        // Order of frequently accessed
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram.peek(addr & 0x07FF), // 0x0800..=0x1FFF are mirrored
            0x4020..=0xFFFF => {
                if let Some(gc) = self.genie_code(addr) {
                    if let Some(compare) = gc.compare {
                        let val = self.mapper.peek(addr);
                        if val == compare {
                            gc.data
                        } else {
                            val
                        }
                    } else {
                        gc.data
                    }
                } else {
                    self.mapper.peek(addr)
                }
            }
            0x4000..=0x4013 | 0x4015 => self.apu.peek(addr),
            0x4016..=0x4017 => self.input.peek(addr),
            0x2000..=0x3FFF => self.ppu.peek(addr & 0x2007), // 0x2008..=0x3FFF are mirrored
            0x4018..=0x401F => self.open_bus,                // APU/IO Test Mode
            0x4014 => self.open_bus,
        }
    }
}

impl MemWrite for Bus {
    fn write(&mut self, addr: Addr, val: Byte) {
        // Some mappers monitor the bus
        self.mapper.open_bus(addr, val);
        self.open_bus = val;
        // Order of frequently accessed
        match addr {
            // Start..End => Read memory
            0x0000..=0x1FFF => self.wram.write(addr & 0x07FF, val), // 0x0800..=0x1FFF are mirrored
            0x4020..=0xFFFF => self.mapper.write(addr, val),
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, val),
            0x4016 => self.input.write(addr, val),
            0x2000..=0x3FFF => {
                self.ppu.write(addr & 0x2007, val); // 0x2008..=0x3FFF are mirrored
                self.mapper.ppu_write(addr, val);
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
        self.mapper.reset();
    }
    fn power_cycle(&mut self) {
        self.apu.power_cycle();
        self.ppu.power_cycle();
        self.mapper.power_cycle();
    }
}

impl Savable for Bus {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.ppu.save(fh)?;
        self.apu.save(fh)?;
        self.mapper.save(fh)?;
        // Ignore input
        self.wram.save(fh)?;
        self.open_bus.save(fh)?;
        // Ignore genie_codes
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.ppu.load(fh)?;
        self.apu.load(fh)?;
        self.mapper.load(fh)?;
        self.ppu.load_mapper(&mut self.mapper);
        self.apu.load_mapper(&mut self.mapper);
        self.wram.load(fh)?;
        self.open_bus.load(fh)?;
        Ok(())
    }
}

impl Default for Bus {
    fn default() -> Self {
        let consistent_ram = true;
        Self::new(consistent_ram)
    }
}

impl fmt::Debug for Bus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO Bus Debug
        write!(f, "Bus {{ }}")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_bus() {
        use super::*;
        use crate::mapper;
        use std::{fs::File, io::BufReader};

        let rom_file = "tests/cpu/nestest.nes";
        let rom = File::open(rom_file).expect("valid file");
        let mut rom = BufReader::new(rom);
        let consistent_ram = true;
        let mapper = mapper::load_rom(rom_file, &mut rom, consistent_ram).expect("loaded mapper");
        let mut mem = Bus::new(consistent_ram);
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
