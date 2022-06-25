use crate::{
    apu::Apu,
    cart::Cart,
    common::{Kind, NesRegion, Reset},
    genie::GenieCode,
    input::Input,
    memory::{MemRead, MemWrite, Memory, RamState},
    ppu::Ppu,
    NesResult,
};
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
    #[serde(skip)]
    pub input: Input,
    pub wram: Memory,
    genie_codes: HashMap<u16, GenieCode>,
    open_bus: u8,
}

impl Bus {
    pub fn new(nes_region: NesRegion, ram_state: RamState) -> Self {
        let mut bus = Self {
            ppu: Ppu::new(nes_region),
            apu: Apu::new(nes_region),
            input: Input::new(nes_region),
            cart: Box::new(Cart::new()),
            wram: Memory::ram(WRAM_SIZE, ram_state),
            genie_codes: HashMap::new(),
            open_bus: 0x00,
        };
        bus.update_cart();
        bus
    }

    #[inline]
    pub fn update_cart(&mut self) {
        self.ppu.load_cart(&mut self.cart);
        self.apu.load_cart(&mut self.cart);
    }

    #[inline]
    pub fn load_cart(&mut self, cart: Cart) {
        self.cart = Box::new(cart);
        self.update_cart();
    }

    /// Add a Game Genie code to override memory reads/writes.
    ///
    /// # Errors
    ///
    /// Errors if genie code is invalid.
    pub fn add_genie_code(&mut self, code: String) -> NesResult<()> {
        let genie_code = GenieCode::new(code)?;
        let addr = genie_code.addr();
        self.genie_codes.insert(addr, genie_code);
        Ok(())
    }

    #[inline]
    pub fn remove_genie_code(&mut self, code: &str) {
        self.genie_codes.retain(|_, gc| gc.code() != code);
    }

    #[inline]
    fn genie_match(&self, addr: u16, val: u8) -> Option<u8> {
        self.genie_codes
            .get(&addr)
            .map(|genie_code| genie_code.matches(val))
    }
}

impl MemRead for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        let val = match addr {
            0x0000..=0x1FFF => self.wram.read(addr & 0x07FF), // 0x0800..=0x1FFF are mirrored
            0x2000..=0x3FFF => self.ppu.read(addr & 0x2007),  // 0x2008..=0x3FFF are mirrored
            0x4020..=0xFFFF => {
                let val = self.cart.read(addr);
                if let Some(data) = self.genie_match(addr, val) {
                    data
                } else {
                    val
                }
            }
            0x4000..=0x4013 | 0x4015 => self.apu.read(addr),
            0x4016 | 0x4017 => self.input.read(addr, &self.ppu),
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
                let val = self.cart.peek(addr);
                if let Some(data) = self.genie_match(addr, val) {
                    data
                } else {
                    val
                }
            }
            0x4000..=0x4013 | 0x4015 => self.apu.peek(addr),
            0x4016 | 0x4017 => self.input.peek(addr, &self.ppu),
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
                self.ppu.oam_dma_offset = val;
            }
            0x4016 => self.input.write(addr, val),
            0x4018..=0x401F => (), // APU/IO Test Mode
        }
    }
}

impl Reset for Bus {
    fn reset(&mut self, kind: Kind) {
        self.apu.reset(kind);
        self.ppu.reset(kind);
        self.cart.reset(kind);
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(NesRegion::default(), RamState::default())
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
        let mut mem = Bus::default();
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
