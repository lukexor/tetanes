//! NES Memory/Data Bus implementation.
//!
//! <https://wiki.nesdev.org/w/index.php/CPU_memory_map>

use crate::{
    apu::{Apu, Channel},
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sample, Sram},
    fs,
    genie::GenieCode,
    input::{Input, InputRegisters, Player},
    mapper::{Map, Mapper},
    mem::{ConstArray, RamState, Read, Write},
    ppu::Ppu,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

/// NES Bus
///
/// <https://wiki.nesdev.org/w/index.php/CPU_memory_map>
///
/// |-----------------| $FFFF |-----------------|
/// | PRG-ROM         |       |                 |
/// |-----------------| $8000 |-----------------|
/// | PRG-RAM or SRAM |       | PRG-RAM or SRAM |
/// |-----------------| $6000 |-----------------|
/// | Expansion       |       | Expansion       |
/// | Modules         |       | Modules         |
/// |-----------------| $4020 |-----------------|
/// | APU/Input       |       |                 |
/// | Registers       |       |                 |
/// |- - - - - - - - -| $4000 |                 |
/// | PPU Mirrors     |       | I/O Registers   |
/// | $2000-$2007     |       |                 |
/// |- - - - - - - - -| $2008 |                 |
/// | PPU Registers   |       |                 |
/// |-----------------| $2000 |-----------------|
/// | WRAM Mirrors    |       |                 |
/// | $0000-$07FF     |       |                 |
/// |- - - - - - - - -| $0800 |                 |
/// | WRAM            |       | 2K Internal     |
/// |- - - - - - - - -| $0200 | Work RAM        |
/// | Stack           |       |                 |
/// |- - - - - - - - -| $0100 |                 |
/// | Zero Page       |       |                 |
/// |-----------------| $0000 |-----------------|
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(C)]
pub struct Bus {
    /// Picture Processing Unit.
    pub ppu: Ppu,
    /// Audio Processing Unit.
    pub apu: Apu,
    /// Joypad and Zapper inputs.
    pub input: Input,
    // 2K NES Work Ram available to the CPU.
    pub wram: Box<ConstArray<u8, { size::WRAM }>>,
    /// Game GENIE codes.
    pub genie_codes: HashMap<u16, GenieCode>,
    /// Whatever was last read or written to to the Bus.
    pub open_bus: u8,
    /// RAM initialization state.
    #[serde(skip)]
    pub ram_state: RamState,
    /// NES Region.
    pub region: NesRegion,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(NesRegion::default(), RamState::default())
    }
}

pub mod size {
    // 2K NES Work Ram available to the CPU.
    pub const WRAM: usize = 0x800;
}

impl Bus {
    pub fn new(region: NesRegion, ram_state: RamState) -> Self {
        Self {
            wram: Box::new(ConstArray::new()),
            ppu: Ppu::new(region),
            apu: Apu::new(region),
            input: Input::new(region),
            genie_codes: HashMap::new(),
            open_bus: 0x00,
            ram_state,
            region,
        }
    }

    pub fn load_cart(&mut self, cart: Cart) {
        self.ppu.load_mapper(cart.mapper);
    }

    pub fn unload_cart(&mut self) {
        self.ppu.load_mapper(Mapper::default());
    }

    #[must_use]
    #[inline]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn wram(&self) -> &[u8; size::WRAM] {
        &self.wram
    }

    /// Add a Game Genie code to override memory reads/writes.
    ///
    /// # Errors
    ///
    /// Errors if genie code is invalid.
    pub fn add_genie_code(&mut self, genie_code: GenieCode) {
        let addr = genie_code.addr();
        self.genie_codes.insert(addr, genie_code);
    }

    /// Remove a Game Genie code.
    pub fn remove_genie_code(&mut self, code: &str) {
        self.genie_codes.retain(|_, gc| gc.code() != code);
    }

    /// Remove all Game Genie codes.
    pub fn clear_genie_codes(&mut self) {
        self.genie_codes.clear();
    }

    fn genie_read(&self, addr: u16, val: u8) -> u8 {
        self.genie_codes
            .get(&addr)
            .map_or(val, |genie_code| genie_code.read(val))
    }

    #[inline]
    #[must_use]
    pub fn audio_samples(&self) -> &[f32] {
        &self.apu.audio_samples
    }

    #[inline]
    pub fn clear_audio_samples(&mut self) {
        self.apu.audio_samples.clear();
    }

    #[inline]
    pub fn cpu_clock(&mut self) {
        self.ppu.mapper.clock();
        let output = self.ppu.mapper.output();
        self.input.clock();
        self.apu.add_mapper_output(output);
        self.apu.clock_lazy();
    }
}

impl Read for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        let addr = match addr {
            0x0800..=0x1FFF => addr & 0x07FF,
            0x2008..=0x3FFF => addr & 0x2007,
            _ => addr,
        };
        self.open_bus = match addr {
            0x0000..=0x07FF => self.wram[usize::from(addr)],
            0x4100..=0xFFFF => {
                let val = self.ppu.mapper.prg_read(addr);
                self.genie_read(addr, val)
            }
            0x2002 => self.ppu.read_status(),
            0x2004 => self.ppu.read_oamdata(),
            0x2007 => self.ppu.read_data(),
            0x4015 => self.apu.read_status(),
            0x4016 => self.input.read(Player::One, &self.ppu),
            0x4017 => self.input.read(Player::Two, &self.ppu),
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.ppu.open_bus,
            _ => self.open_bus,
        };
        self.open_bus
    }

    fn peek(&self, addr: u16) -> u8 {
        let addr = match addr {
            0x0800..=0x1FFF => addr & 0x07FF,
            0x2008..=0x3FFF => addr & 0x2007,
            _ => addr,
        };
        match addr {
            0x0000..=0x07FF => self.wram[usize::from(addr)],
            0x4100..=0xFFFF => {
                let val = self.ppu.mapper.prg_peek(addr);
                self.genie_read(addr, val)
            }
            0x2002 => self.ppu.peek_status(),
            0x2004 => self.ppu.peek_oamdata(),
            0x2007 => self.ppu.peek_data(),
            0x4015 => self.apu.peek_status(),
            0x4016 => self.input.peek(Player::One, &self.ppu),
            0x4017 => self.input.peek(Player::Two, &self.ppu),
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.ppu.open_bus,
            _ => self.open_bus,
        }
    }
}

impl Write for Bus {
    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        let addr = match addr {
            0x0800..=0x1FFF => addr & 0x07FF,
            0x2008..=0x3FFF => addr & 0x2007,
            _ => addr,
        };
        match addr {
            0x0000..=0x07FF => self.wram[usize::from(addr)] = val,
            0x4100..=0xFFFF => self.ppu.mapper.prg_write(addr, val),
            0x2000 => self.ppu.write_ctrl(val),
            0x2001 => self.ppu.write_mask(val),
            0x2002 => self.ppu.open_bus = val,
            0x2003 => self.ppu.write_oamaddr(val),
            0x2004 => self.ppu.write_oamdata(val),
            0x2005 => self.ppu.write_scroll(val),
            0x2006 => self.ppu.write_addr(val),
            0x2007 => self.ppu.write_data(val),
            0x4000 => self.apu.write_ctrl(Channel::Pulse1, val),
            0x4001 => self.apu.write_sweep(Channel::Pulse1, val),
            0x4002 => self.apu.write_timer_lo(Channel::Pulse1, val),
            0x4003 => self.apu.write_timer_hi(Channel::Pulse1, val),
            0x4004 => self.apu.write_ctrl(Channel::Pulse2, val),
            0x4005 => self.apu.write_sweep(Channel::Pulse2, val),
            0x4006 => self.apu.write_timer_lo(Channel::Pulse2, val),
            0x4007 => self.apu.write_timer_hi(Channel::Pulse2, val),
            0x4008 => self.apu.write_linear_counter(val),
            0x400A => self.apu.write_timer_lo(Channel::Triangle, val),
            0x400B => self.apu.write_timer_hi(Channel::Triangle, val),
            0x400C => self.apu.write_ctrl(Channel::Noise, val),
            0x400E => self.apu.write_timer_lo(Channel::Noise, val),
            0x400F => self.apu.write_length(Channel::Noise, val),
            0x4010 => self.apu.write_timer_lo(Channel::Dmc, val),
            0x4011 => self.apu.write_dmc_output(val),
            0x4012 => self.apu.write_dmc_addr(val),
            0x4013 => self.apu.write_length(Channel::Dmc, val),
            0x4015 => self.apu.write_status(val),
            0x4016 => self.input.write(val),
            0x4017 => self.apu.write_frame_counter(val),
            0x4014 => (), // DMA handled by CPU
            _ => (),
        }
    }
}

impl Regional for Bus {
    fn region(&self) -> NesRegion {
        self.region
    }

    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.ppu.set_region(region);
        self.apu.set_region(region);
        self.input.set_region(region);
    }
}

impl Reset for Bus {
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            self.ram_state.fill(&mut **self.wram);
        }
        self.ppu.reset(kind);
        self.apu.reset(kind);
    }
}

impl Sram for Bus {
    fn save(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        self.ppu.mapper.save(path)
    }

    fn load(&mut self, path: impl AsRef<Path>) -> fs::Result<()> {
        self.ppu.mapper.load(path)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        mapper::{Cnrom, Nrom},
        mem::Memory,
    };

    #[test]
    fn load_cart_values() {
        let mut bus = Bus::default();
        #[rustfmt::skip]
        let rom: [u8; 16] = [
            0x4E, 0x45, 0x53, 0x1A,
            0x00, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];
        let cart = Cart::from_rom("load_cart_test", &mut rom.as_slice(), RamState::default())
            .expect("valid cart");

        let expected_mirroring = cart.mirroring();
        let expected_region = cart.region();
        bus.load_cart(cart);

        assert_eq!(bus.ppu.region(), expected_region, "ppu region");
        assert_eq!(bus.apu.region(), expected_region, "apu region");
        assert!(
            matches!(bus.ppu.mapper, Mapper::Nrom(_)),
            "mapper is Nrom: {:?}",
            bus.ppu.mapper
        );
        assert_eq!(bus.ppu.mirroring(), expected_mirroring, "mirroring");
    }

    #[test]
    fn load_cart_chr_rom() {
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        let mut chr_rom = Memory::new(0x2000);
        chr_rom.fill(0x66);
        // Cnrom doesn't provide CHR-RAM
        cart.mapper = Cnrom::load(&cart, chr_rom, Memory::new(0x4000)).unwrap();
        bus.load_cart(cart);

        bus.write(0x2006, 0x00);
        bus.write(0x2006, 0x00);
        bus.read(0x2007);
        assert_eq!(bus.read(0x2007), 0x66, "chr_rom start");
        bus.write(0x2006, 0x1F);
        bus.write(0x2006, 0xFF);
        bus.read(0x2007);
        assert_eq!(bus.read(0x2007), 0x66, "chr_rom end");

        // Writes disallowed
        bus.write(0x2006, 0x00);
        bus.write(0x2006, 0x10);
        bus.write(0x2007, 0x77);

        bus.write(0x2006, 0x00);
        bus.write(0x2006, 0x10);
        bus.read(0x2007);
        assert_eq!(bus.read(0x2007), 0x66, "chr_rom read-only");
    }

    #[test]
    fn load_cart_chr_ram() {
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        cart.mapper = Nrom::load(&cart, Memory::empty(), Memory::new(cart.prg_rom_size)).unwrap();
        if let Mapper::Nrom(nrom) = &mut cart.mapper {
            nrom.chr.fill(0x66);
        }
        bus.load_cart(cart);

        bus.write(0x2006, 0x00);
        bus.write(0x2006, 0x00);
        bus.read(0x2007);
        assert_eq!(bus.read(0x2007), 0x66, "chr_ram start");
        bus.write(0x2006, 0x1F);
        bus.write(0x2006, 0xFF);
        bus.read(0x2007);
        assert_eq!(bus.read(0x2007), 0x66, "chr_ram end");

        // Writes allowed
        bus.write(0x2006, 0x10);
        bus.write(0x2006, 0x00);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu.clock();
        bus.ppu.clock();
        bus.write(0x2007, 0x77);

        bus.write(0x2006, 0x10);
        bus.write(0x2006, 0x00);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu.clock();
        bus.ppu.clock();
        bus.read(0x2007);
        assert_eq!(bus.read(0x2007), 0x77, "chr_ram write");
    }

    #[test]
    fn genie_codes() {
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        let mut prg_rom = Memory::new(0x8000);

        let code = "YYKPOYZZ"; // The Legend of Zelda: New character with 8 Hearts
        let addr = 0x9F41;
        let orig_value = 0x22; // 3 Hearts
        let new_value = 0x77; // 8 Hearts

        prg_rom[(addr & 0x7FFF) as usize] = orig_value;
        cart.mapper = Nrom::load(&cart, Memory::new(cart.chr_rom_size), prg_rom).unwrap();

        bus.load_cart(cart);
        bus.add_genie_code(GenieCode::new(code.to_string()).expect("valid genie code"));

        assert_eq!(bus.peek(addr), new_value, "peek code value");
        assert_eq!(bus.read(addr), new_value, "read code value");
        bus.remove_genie_code(code);
        assert_eq!(bus.peek(addr), orig_value, "peek orig value");
        assert_eq!(bus.read(addr), orig_value, "read orig value");
    }

    #[test]
    fn clock() {
        let mut bus = Bus::default();

        bus.ppu.clock_to(12);
        assert_eq!(bus.ppu.master_clock, 12, "ppu clock");
        bus.cpu_clock();
        assert_eq!(bus.apu.master_clock, 1, "apu clock");
    }

    #[test]
    fn read_write_ram() {
        let mut bus = Bus::default();

        bus.write(0x0001, 0x66);
        assert_eq!(bus.peek(0x0001), 0x66, "peek ram");
        assert_eq!(bus.read(0x0001), 0x66, "read ram");
        assert_eq!(bus.read(0x0801), 0x66, "peek mirror 1");
        assert_eq!(bus.read(0x0801), 0x66, "read mirror 1");
        assert_eq!(bus.read(0x1001), 0x66, "peek mirror 2");
        assert_eq!(bus.read(0x1001), 0x66, "read mirror 2");
        assert_eq!(bus.read(0x1801), 0x66, "peek mirror 3");
        assert_eq!(bus.read(0x1801), 0x66, "read mirror 3");

        bus.write(0x0802, 0x77);
        assert_eq!(bus.read(0x0002), 0x77, "write mirror 1");
        bus.write(0x1002, 0x88);
        assert_eq!(bus.read(0x0002), 0x88, "write mirror 2");
        bus.write(0x1802, 0x99);
        assert_eq!(bus.read(0x0002), 0x99, "write mirror 3");
    }

    #[test]
    #[ignore = "todo"]
    fn read_write_ppu() {
        // read: PPUSTATUS, OAMDATA, PPUDATA + Mirrors
        // peek: PPUSTATUS, OAMDATA, PPUDATA + Mirrors
        // write: PPUCTRL, PPUMASK, OAMADDR, OAMDATA, PPUSCROLL, PPUADDR, PPUDATA + Mirrors
        todo!()
    }

    #[test]
    #[ignore = "todo"]
    fn read_write_apu() {
        // read: APU_STATUS
        // write: APU_STATUS, APU_FRAME_COUNTER
        todo!()
    }

    #[test]
    #[ignore = "todo"]
    fn write_apu_pulse() {
        // write: APU_CTRL_PULSE1, APU_SWEEP_PULSE1, APU_TIMER_LO_PULSE1, APU_TIMER_HI_PULSE1
        // write: APU_CTRL_PULSE2, APU_SWEEP_PULSE2, APU_TIMER_LO_PULSE2, APU_TIMER_HI_PULSE2
        todo!();
    }

    #[test]
    #[ignore = "todo"]
    fn write_apu_triangle() {
        // write: APU_LIN_CTR_TRIANGLE, APU_TIMER_LO_TRIANGLE, APU_TIMER_HI_TRIANGLE
        todo!();
    }

    #[test]
    #[ignore = "todo"]
    fn write_apu_noise() {
        // write: APU_CTRL_NOISE, APU_TIMER_NOISE, APU_LENGTH_NOISE
        todo!()
    }

    #[test]
    #[ignore = "todo"]
    fn write_dmc() {
        // write: APU_TIMER_DMC, APU_OUTPUT_DMC, APU_ADDR_LOAD_DMC, APU_LENGTH_DMC
        todo!()
    }

    #[test]
    #[ignore = "todo"]
    fn read_write_input() {
        todo!()
    }

    #[test]
    #[ignore = "todo"]
    fn read_write_mapper() {
        todo!()
    }

    #[test]
    #[ignore = "todo"]
    fn reset() {
        todo!()
    }
}
