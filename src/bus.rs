use crate::{
    apu::Apu,
    cart::Cart,
    common::Powered,
    hashmap,
    input::Input,
    memory::{MemRead, MemWrite, Memory, RamState},
    ppu::Ppu,
    NesResult,
};
use anyhow::anyhow;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};

const WRAM_SIZE: usize = 2 * 1024; // 2K NES Work Ram

/// NES Bus
///
/// <http://wiki.nesdev.com/w/index.php/CPU_memory_map>
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bus {
    pub ppu: Ppu,
    pub apu: Apu,
    pub cart: Box<Cart>,
    pub input: Input,
    pub wram: Memory,
    pub halt: bool,
    pub dummy_read: bool,
    genie_codes: HashMap<u16, GenieCode>,
    open_bus: u8,
}

/// Game Genie Code
#[derive(Clone, Serialize, Deserialize)]
struct GenieCode {
    code: String,
    data: u8,
    compare: Option<u8>,
}

impl fmt::Debug for GenieCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.code)
    }
}

lazy_static! {
    static ref GENIE_MAP: HashMap<char, u8> = {
        // Game genie maps these letters to binary representations as a form of code obfuscation
        hashmap! {
            'A' => 0x0, 'P' => 0x1, 'Z' => 0x2, 'L' => 0x3, 'G' => 0x4, 'I' => 0x5, 'T' => 0x6,
            'Y' => 0x7, 'E' => 0x8, 'O' => 0x9, 'X' => 0xA, 'U' => 0xB, 'K' => 0xC, 'S' => 0xD,
            'V' => 0xE, 'N' => 0xF
        }
    };
}

impl Bus {
    pub fn new(power_state: RamState) -> Self {
        let mut bus = Self {
            ppu: Ppu::new(),
            apu: Apu::new(),
            input: Input::new(),
            cart: Box::new(Cart::new()),
            wram: Memory::ram(WRAM_SIZE, power_state),
            halt: false,
            dummy_read: false,
            genie_codes: HashMap::new(),
            open_bus: 0,
        };
        bus.ppu.load_cart(&mut bus.cart);
        bus.apu.load_cart(&mut bus.cart);
        bus
    }

    #[inline]
    pub fn load_cart(&mut self, cart: Cart) {
        self.cart = Box::new(cart);
        self.ppu.load_cart(&mut self.cart);
        self.apu.load_cart(&mut self.cart);
    }

    /// Add a Game Genie code to override memory reads/writes.
    ///
    /// # Errors
    ///
    /// Errors if genie code is invalid.
    pub fn add_genie_code(&mut self, code: &str) -> NesResult<()> {
        if code.len() != 6 && code.len() != 8 {
            return Err(anyhow!("invalid game genie code: {}", code));
        }
        let mut hex: Vec<u8> = Vec::with_capacity(code.len());
        for s in code.chars() {
            if let Some(h) = GENIE_MAP.get(&s) {
                hex.push(*h);
            } else {
                return Err(anyhow!("invalid game genie code: {}", code));
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

    #[inline]
    pub fn remove_genie_code(&mut self, code: &str) {
        self.genie_codes.retain(|_, gc| gc.code != code);
    }

    #[inline]
    fn genie_code(&self, addr: u16) -> Option<GenieCode> {
        if self.genie_codes.is_empty() {
            None
        } else {
            self.genie_codes.get(&addr).cloned()
        }
    }
}

impl MemRead for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        let val = match addr {
            0x0000..=0x1FFF => self.wram.read(addr & 0x07FF), // 0x0800..=0x1FFF are mirrored
            0x2000..=0x3FFF => self.ppu.read(addr & 0x2007),  // 0x2008..=0x3FFF are mirrored
            0x4020..=0xFFFF => {
                let gc = self.genie_code(addr);
                if let Some(gc) = gc {
                    if let Some(compare) = gc.compare {
                        let val = self.cart.read(addr);
                        if val == compare {
                            gc.data
                        } else {
                            val
                        }
                    } else {
                        gc.data
                    }
                } else {
                    self.cart.read(addr)
                }
            }
            0x4000..=0x4013 | 0x4015 => self.apu.read(addr),
            0x4016 => self.input.read(addr),
            0x4017 => {
                if self.input.zapper.connected {
                    self.input.read_zapper(&self.ppu)
                } else {
                    self.input.read(addr)
                }
            }
            0x4014 | 0x4018..=0x401F => self.open_bus, // APU/IO Test Mode
        };
        self.cart.bus_read(val);
        self.open_bus = val;
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.wram.peek(addr & 0x07FF), // 0x0800..=0x1FFF are mirrored
            0x2000..=0x3FFF => self.ppu.peek(addr & 0x2007),  // 0x2008..=0x3FFF are mirrored
            0x4020..=0xFFFF => {
                if let Some(gc) = self.genie_code(addr) {
                    if let Some(compare) = gc.compare {
                        let val = self.cart.peek(addr);
                        if val == compare {
                            gc.data
                        } else {
                            val
                        }
                    } else {
                        gc.data
                    }
                } else {
                    self.cart.peek(addr)
                }
            }
            0x4000..=0x4013 | 0x4015 => self.apu.peek(addr),
            0x4016 => self.input.peek(addr),
            0x4017 => {
                if self.input.zapper.connected {
                    self.input.read_zapper(&self.ppu)
                } else {
                    self.input.peek(addr)
                }
            }
            0x4014 | 0x4018..=0x401F => self.open_bus, // APU/IO Test Mode
        }
    }
}

impl MemWrite for Bus {
    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        match addr {
            0x0000..=0x1FFF => self.wram.write(addr & 0x07FF, val), // 0x0800..=0x1FFF are mirrored
            0x2000..=0x3FFF => self.ppu.write(addr & 0x2007, val),  // 0x2008..=0x3FFF are mirrored
            0x4020..=0xFFFF => self.cart.write(addr, val),
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, val),
            0x4014 => {
                self.ppu.oam_dma = true;
                self.ppu.dma_offset = val;
                self.halt = true;
            }
            0x4016 => self.input.write(addr, val),
            0x4018..=0x401F => (), // APU/IO Test Mode
        }
    }
}

impl Powered for Bus {
    fn reset(&mut self) {
        self.apu.reset();
        self.ppu.reset();
        self.cart.reset();
        self.halt = false;
        self.dummy_read = false;
    }
    fn power_cycle(&mut self) {
        self.apu.power_cycle();
        self.ppu.power_cycle();
        self.cart.power_cycle();
        self.halt = false;
        self.dummy_read = false;
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(RamState::AllZeros)
    }
}

impl fmt::Debug for Bus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bus")
            .field("ppu", &self.ppu)
            .field("apu", &self.apu)
            .field("cart", &self.cart)
            .field("input", &self.input)
            .field("wram", &self.wram)
            .field("halt", &self.halt)
            .field("dummy_read", &self.dummy_read)
            .field("genie_codes", &self.genie_codes.values())
            .field("open_bus", &format_args!("${:02X}", &self.open_bus))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_bus() {
        use super::*;
        use crate::cart::Cart;
        use std::{fs::File, io::BufReader};

        let rom_file = "test_roms/cpu/nestest.nes";
        let rom = File::open(rom_file).expect("valid file");
        let mut rom = BufReader::new(rom);
        let cart = Cart::from_rom(&rom_file, &mut rom, RamState::AllZeros).expect("loaded cart");
        let mut mem = Bus::new(RamState::AllZeros);
        mem.load_cart(cart);
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
