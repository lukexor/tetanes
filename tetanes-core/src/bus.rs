//! NES Memory/Data Bus implementation.
//!
//! <https://wiki.nesdev.org/w/index.php/CPU_memory_map>

use crate::{
    apu::{Apu, ApuRegisters, Channel},
    cart::Cart,
    common::{Clock, ClockTo, NesRegion, Regional, Reset, ResetKind, Sample, Sram},
    cpu::Cpu,
    fs,
    genie::GenieCode,
    input::{Input, InputRegisters, Player},
    mapper::{BusKind, MapRead, MapWrite, MappedRead, MappedWrite, Mapper, OnBusRead, OnBusWrite},
    mem::{ConstSlice, Memory, RamState, Read, Write},
    ppu::{Ppu, Registers},
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
pub struct Bus {
    pub apu: Apu,
    pub genie_codes: HashMap<u16, GenieCode>,
    pub input: Input,
    pub open_bus: u8,
    pub ppu: Ppu,
    pub prg_ram_protect: bool,
    pub prg_ram: Memory<Vec<u8>>,
    #[serde(skip)]
    pub prg_rom: Memory<Vec<u8>>,
    pub ram_state: RamState,
    pub region: NesRegion,
    pub wram: Memory<ConstSlice<u8, 0x0800>>, // 2K NES Work Ram available to the CPU
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(NesRegion::Ntsc, RamState::default())
    }
}

impl Bus {
    pub fn new(region: NesRegion, ram_state: RamState) -> Self {
        Self {
            apu: Apu::new(region),
            genie_codes: HashMap::new(),
            input: Input::new(region),
            open_bus: 0x00,
            ppu: Ppu::new(region, ram_state),
            prg_ram: Memory::ram(ram_state),
            prg_ram_protect: false,
            prg_rom: Memory::rom(),
            ram_state,
            region,
            wram: Memory::ram_const(ram_state),
        }
    }

    pub fn load_cart(&mut self, cart: Cart) {
        self.prg_rom = cart.prg_rom;
        self.load_sram(cart.prg_ram);
        if cart.chr_ram.is_empty() {
            self.ppu.bus.load_chr(cart.chr_rom);
        } else {
            self.ppu.bus.load_chr(cart.chr_ram);
        }
        self.ppu.bus.load_ex_ram(cart.ex_ram);
        self.ppu.load_mapper(cart.mapper);
    }

    pub fn unload_cart(&mut self) {
        self.ppu.load_mapper(Mapper::default());
    }

    #[must_use]
    #[inline]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn sram(&self) -> &[u8] {
        &self.prg_ram
    }

    #[inline]
    pub fn load_sram(&mut self, sram: impl Into<Memory<Vec<u8>>>) {
        self.prg_ram = sram.into();
    }

    #[must_use]
    #[inline]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn wram(&self) -> &[u8] {
        self.wram.as_ref()
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

    #[must_use]
    #[inline]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn audio_samples(&self) -> &[f32] {
        &self.apu.audio_samples
    }

    #[inline]
    pub fn clear_audio_samples(&mut self) {
        self.apu.audio_samples.clear();
    }
}

impl Clock for Bus {
    fn clock(&mut self) -> u64 {
        self.ppu.bus.mapper.clock();
        let output = match self.ppu.bus.mapper {
            Mapper::Exrom(ref exrom) => exrom.output(),
            Mapper::Namco163(ref namco163) => namco163.output(),
            Mapper::Vrc6(ref vrc6) => vrc6.output(),
            Mapper::SunsoftFme7(ref sunsoft_fme7) => sunsoft_fme7.output(),
            _ => 0.0,
        };
        self.apu.add_mapper_output(output);
        self.apu.clock_lazy();
        self.input.clock();

        1
    }
}

impl ClockTo for Bus {
    fn clock_to(&mut self, clock: u64) -> u64 {
        self.ppu.clock_to(clock)
    }
}

impl Read for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        let val = match addr {
            0x0000..=0x07FF => self.wram[addr as usize],
            0x4020..=0xFFFF => {
                let val = match self.ppu.bus.mapper.map_read(addr) {
                    MappedRead::Data(val) => val,
                    MappedRead::PrgRam(addr) => self.prg_ram[addr],
                    MappedRead::PrgRom(addr) => self.prg_rom[addr],
                    _ => self.open_bus,
                };
                self.genie_read(addr, val)
            }
            0x2002 => self.ppu.read_status(),
            0x2004 => self.ppu.read_oamdata(),
            0x2007 => self.ppu.read_data(),
            0x4015 => self.apu.read_status(),
            0x4016 => self.input.read(Player::One, &self.ppu),
            0x4017 => self.input.read(Player::Two, &self.ppu),
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.ppu.open_bus,
            0x0800..=0x1FFF => self.read(addr & 0x07FF), // WRAM Mirrors
            0x2008..=0x3FFF => self.read(addr & 0x2007), // Ppu Mirrors
            _ => self.open_bus,
        };
        self.open_bus = val;
        self.ppu.bus.mapper.on_bus_read(addr, BusKind::Cpu);
        val
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x07FF => self.wram[addr as usize],
            0x4020..=0xFFFF => {
                let val = match self.ppu.bus.mapper.map_peek(addr) {
                    MappedRead::Data(val) => val,
                    MappedRead::PrgRam(addr) => self.prg_ram[addr],
                    MappedRead::PrgRom(addr) => self.prg_rom[addr],
                    _ => self.open_bus,
                };
                self.genie_read(addr, val)
            }
            0x2002 => self.ppu.peek_status(),
            0x2004 => self.ppu.peek_oamdata(),
            0x2007 => self.ppu.peek_data(),
            0x4015 => self.apu.peek_status(),
            0x4016 => self.input.peek(Player::One, &self.ppu),
            0x4017 => self.input.peek(Player::Two, &self.ppu),
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.ppu.open_bus,
            0x0800..=0x1FFF => self.peek(addr & 0x07FF), // WRAM Mirrors
            0x2008..=0x3FFF => self.peek(addr & 0x2007), // Ppu Mirrors
            _ => self.open_bus,
        }
    }
}

impl Write for Bus {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x07FF => {
                self.wram[addr as usize] = val;
            }
            0x4020..=0xFFFF => match self.ppu.bus.mapper.map_write(addr, val) {
                MappedWrite::PrgRam(addr, val) => {
                    if !self.prg_ram.is_empty() && !self.prg_ram_protect {
                        self.prg_ram[addr] = val;
                    }
                }
                MappedWrite::PrgRamProtect(protect) => self.prg_ram_protect = protect,
                _ => (),
            },
            0x2000 => self.ppu.write_ctrl(val),
            0x2001 => self.ppu.write_mask(val),
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
            0x4014 => Cpu::start_oam_dma(u16::from(val) << 8),
            0x4015 => self.apu.write_status(val),
            0x4016 => self.input.write(val),
            0x4017 => self.apu.write_frame_counter(val),
            0x2002 => self.ppu.open_bus = val,
            0x0800..=0x1FFF => return self.write(addr & 0x07FF, val), // WRAM Mirrors
            0x2008..=0x3FFF => return self.write(addr & 0x2007, val), // Ppu Mirrors
            _ => (),
        }
        self.open_bus = val;
        self.ppu.bus.mapper.on_bus_write(addr, val, BusKind::Cpu);
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
        self.prg_ram.reset(kind);
        self.wram.reset(kind);
        self.ppu.reset(kind);
        self.apu.reset(kind);
    }
}

impl Sram for Bus {
    fn save(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        fs::save(path.as_ref(), self.sram())?;
        self.ppu.bus.mapper.save(path)
    }

    fn load(&mut self, path: impl AsRef<Path>) -> fs::Result<()> {
        fs::load(path.as_ref()).map(|mut data: Memory<Vec<u8>>| {
            data.set_ram(self.ram_state);
            self.load_sram(data)
        })?;
        self.ppu.bus.mapper.load(path)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::mapper::Cnrom;

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
            matches!(bus.ppu.bus.mapper, Mapper::Nrom(_)),
            "mapper is Nrom: {:?}",
            bus.ppu.bus.mapper
        );
        assert_eq!(bus.ppu.bus.mirroring(), expected_mirroring, "mirroring");
    }

    #[test]
    fn load_cart_chr_rom() {
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        cart.chr_rom = Memory::rom().with_size(0x2000);
        cart.chr_rom.fill(0x66);
        // Cnrom doesn't provide CHR-RAM
        cart.mapper = Cnrom::load(&mut cart).unwrap();
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
        cart.chr_ram = Memory::ram(RamState::default()).with_size(0x2000);
        cart.chr_ram.fill(0x66);
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

        let code = "YYKPOYZZ"; // The Legend of Zelda: New character with 8 Hearts
        let addr = 0x9F41;
        let orig_value = 0x22; // 3 Hearts
        let new_value = 0x77; // 8 Hearts
        cart.prg_rom[(addr & 0x7FFF) as usize] = orig_value;

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

        bus.clock_to(12);
        assert_eq!(bus.ppu.master_clock, 12, "ppu clock");
        bus.clock();
        assert_eq!(bus.apu.master_cycle, 1, "apu clock");
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
