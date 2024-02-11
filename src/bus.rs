use crate::{
    apu::{Apu, ApuRegisters, Channel},
    audio::Audio,
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind},
    cpu::{Cpu, Irq},
    genie::GenieCode,
    input::{Input, InputRegisters, Player},
    mapper::{Mapped, MappedRead, MappedWrite, Mapper, MemMap},
    mem::{Access, Mem, RamState},
    ppu::{Ppu, PpuRegisters},
    NesResult,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// NES Bus
///
/// <http://wiki.nesdev.com/w/index.php/CPU_memory_map>
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
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Bus {
    wram: Vec<u8>,
    region: NesRegion,
    ram_state: RamState,
    prg_ram: Vec<u8>,
    prg_ram_protect: bool,
    prg_rom: Vec<u8>,
    pub ppu: Ppu,
    pub apu: Apu,
    #[serde(skip)]
    pub input: Input,
    oam_dma: bool,
    oam_dma_addr: u16,
    audio_samples: Vec<f32>,
    genie_codes: HashMap<u16, GenieCode>,
    cycle: usize, // Total number of CPU cycles ran
    open_bus: u8,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(RamState::default())
    }
}

impl Bus {
    const WRAM_SIZE: usize = 0x0800; // 2K NES Work Ram available to the CPU

    pub fn new(ram_state: RamState) -> Self {
        let mut wram = vec![0x00; Self::WRAM_SIZE];
        RamState::fill(&mut wram, ram_state);
        Self {
            wram,
            region: NesRegion::default(),
            ram_state,
            prg_ram: vec![],
            prg_ram_protect: false,
            prg_rom: vec![],
            ppu: Ppu::new(),
            apu: Apu::new(),
            input: Input::new(),
            oam_dma: false,
            oam_dma_addr: 0x0000,
            audio_samples: vec![],
            genie_codes: HashMap::new(),
            cycle: 0,
            open_bus: 0x00,
        }
    }

    pub fn load_cart(&mut self, cart: Cart) {
        // Start with ~20ms of audio capacity
        let audio_capacity = (Cpu::region_clock_rate(cart.region()) * 0.02) as usize;
        if audio_capacity > self.audio_samples.capacity() {
            self.audio_samples
                .reserve(audio_capacity - self.audio_samples.capacity());
        }
        self.set_region(cart.region());
        self.prg_rom = cart.prg_rom;
        self.load_sram(cart.prg_ram);
        self.ppu.bus.load_chr(cart.chr);
        self.ppu.bus.load_ex_ram(cart.ex_ram);
        self.ppu.load_mapper(cart.mapper);
    }

    pub fn unload_cart(&mut self) {
        self.prg_rom.clear();
        self.load_sram(vec![]);
        self.ppu.bus.load_chr(vec![]);
        self.ppu.bus.load_ex_ram(vec![]);
        self.ppu.load_mapper(Mapper::default());
    }

    #[inline]
    #[must_use]
    pub fn sram(&self) -> &[u8] {
        &self.prg_ram
    }

    #[inline]
    pub fn load_sram(&mut self, sram: Vec<u8>) {
        self.prg_ram = sram;
    }

    #[inline]
    #[must_use]
    pub fn wram(&self) -> &[u8] {
        &self.wram
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
    fn genie_read(&self, addr: u16, val: u8) -> u8 {
        self.genie_codes
            .get(&addr)
            .map_or(val, |genie_code| genie_code.read(val))
    }

    #[inline]
    fn mix_audio(&mut self, sample1: f32, sample2: f32) {
        self.audio_samples.push(sample1 + sample2);
    }

    #[inline]
    #[must_use]
    pub fn audio_samples(&self) -> &[f32] {
        &self.audio_samples
    }

    #[inline]
    pub fn clear_audio_samples(&mut self) {
        self.audio_samples.clear();
    }

    #[inline]
    pub fn irqs_pending(&self) -> Irq {
        let mut irq = Irq::empty();
        irq.set(Irq::MAPPER, self.ppu.bus.mapper.irq_pending());
        irq |= self.apu.irqs_pending();
        irq
    }

    #[inline]
    #[must_use]
    pub const fn oam_dma(&self) -> bool {
        self.oam_dma
    }

    #[inline]
    #[must_use]
    pub const fn oam_dma_addr(&self) -> u16 {
        self.oam_dma_addr
    }

    #[inline]
    pub fn oam_dma_finish(&mut self) {
        self.oam_dma = false;
    }
}

impl Clock for Bus {
    fn clock(&mut self) -> usize {
        self.cycle = self.cycle.wrapping_add(1);
        self.apu.clock();
        self.ppu.bus.mapper.clock();
        self.input.clock();

        let apu_output = self.apu.output();
        let mapper_output = match self.ppu.bus.mapper {
            Mapper::Exrom(ref exrom) => exrom.output(),
            Mapper::Vrc6(ref vrc6) => vrc6.output(),
            _ => 0.0,
        };
        self.mix_audio(apu_output, mapper_output);

        1
    }

    #[inline]
    fn clock_to(&mut self, clock: u64) {
        self.ppu.clock_to(clock);
    }
}

impl Mem for Bus {
    fn read(&mut self, addr: u16, _access: Access) -> u8 {
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
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.ppu.open_bus(),
            0x0800..=0x1FFF => self.read(addr & 0x07FF, _access), // WRAM Mirrors
            0x2008..=0x3FFF => self.read(addr & 0x2007, _access), // Ppu Mirrors
            _ => self.open_bus,
        };
        self.open_bus = val;
        self.ppu.bus.mapper.cpu_bus_read(addr);
        val
    }

    fn peek(&self, addr: u16, _access: Access) -> u8 {
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
            0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => self.ppu.open_bus(),
            0x0800..=0x1FFF => self.peek(addr & 0x07FF, _access), // WRAM Mirrors
            0x2008..=0x3FFF => self.peek(addr & 0x2007, _access), // Ppu Mirrors
            _ => self.open_bus,
        }
    }

    fn write(&mut self, addr: u16, val: u8, _access: Access) {
        match addr {
            0x0000..=0x07FF => self.wram[addr as usize] = val,
            0x4020..=0xFFFF => {
                match self.ppu.bus.mapper.map_write(addr, val) {
                    MappedWrite::PrgRam(addr, val) => {
                        let prg_ram_enabled = !self.prg_ram.is_empty() && !self.prg_ram_protect;
                        self.prg_ram[addr] = val;
                    }
                    MappedWrite::PrgRamProtect(protect) => self.prg_ram_protect = protect,
                    _ => (),
                }
                self.ppu.bus.update_mirroring();
            }
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
            0x4008 => self.apu.write_linear_counter(Channel::Triangle, val),
            0x400A => self.apu.write_timer_lo(Channel::Triangle, val),
            0x400B => self.apu.write_timer_hi(Channel::Triangle, val),
            0x400C => self.apu.write_ctrl(Channel::Noise, val),
            0x400E => self.apu.write_timer_lo(Channel::Noise, val),
            0x400F => self.apu.write_length(Channel::Noise, val),
            0x4010 => self.apu.write_timer_lo(Channel::Dmc, val),
            0x4011 => self.apu.write_output(Channel::Dmc, val),
            0x4012 => self.apu.write_addr_load(Channel::Dmc, val),
            0x4013 => self.apu.write_length(Channel::Dmc, val),
            0x4014 => {
                self.oam_dma = true;
                self.oam_dma_addr = u16::from(val) << 8;
            }
            0x4015 => self.apu.write_status(val),
            0x4016 => self.input.write(val),
            0x4017 => self.apu.write_frame_counter(val),
            0x2002 => self.ppu.set_open_bus(val),
            0x0800..=0x1FFF => return self.write(addr & 0x07FF, val, _access), // WRAM Mirrors
            0x2008..=0x3FFF => return self.write(addr & 0x2007, val, _access), // Ppu Mirrors
            _ => (),
        }
        self.open_bus = val;
        self.ppu.bus.mapper.cpu_bus_write(addr, val);
    }
}

impl Regional for Bus {
    #[inline]
    fn region(&self) -> NesRegion {
        self.region
    }

    #[inline]
    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.ppu.set_region(region);
        self.apu.set_region(region);
    }
}

impl Reset for Bus {
    fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            RamState::fill(&mut self.wram, self.ram_state);
        }
        self.ppu.reset(kind);
        self.apu.reset(kind);
    }
}

impl std::fmt::Debug for Bus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bus")
            .field("wram_len", &self.wram.len())
            .field("region", &self.region)
            .field("ram_state", &self.ram_state)
            .field("prg_ram_len", &self.prg_ram.len())
            .field("prg_ram_protect", &self.prg_ram_protect)
            .field("prg_rom_len", &self.prg_rom.len())
            .field("ppu", &self.ppu)
            .field("apu", &self.apu)
            .field("input", &self.input)
            .field("oam_dma", &self.oam_dma)
            .field("oam_dma_addr", &self.oam_dma_addr)
            .field("audio_samples_len", &self.audio_samples.len())
            .field("genie_codes", &self.genie_codes.values())
            .field("cycle", &self.cycle)
            .field("open_bus", &format_args!("${:02X}", &self.open_bus))
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cart::Cart;

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
        cart.chr = vec![0x66; 0x2000];
        bus.load_cart(cart);

        bus.write(0x2006, 0x00, Access::Write);
        bus.write(0x2006, 0x00, Access::Write);
        bus.read(0x2007, Access::Read);
        assert_eq!(bus.read(0x2007, Access::Read), 0x66, "chr_rom start");
        bus.write(0x2006, 0x1F, Access::Write);
        bus.write(0x2006, 0xFF, Access::Write);
        bus.read(0x2007, Access::Read);
        assert_eq!(bus.read(0x2007, Access::Read), 0x66, "chr_rom end");

        // Writes disallowed
        bus.write(0x2006, 0x00, Access::Write);
        bus.write(0x2006, 0x10, Access::Write);
        bus.write(0x2007, 0x77, Access::Write);

        bus.write(0x2006, 0x00, Access::Write);
        bus.write(0x2006, 0x10, Access::Write);
        bus.read(0x2007, Access::Read);
        assert_eq!(bus.read(0x2007, Access::Read), 0x66, "chr_rom read-only");
    }

    #[test]
    fn load_cart_chr_ram() {
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        cart.chr = vec![0x66; 0x2000];
        bus.load_cart(cart);

        bus.write(0x2006, 0x00, Access::Write);
        bus.write(0x2006, 0x00, Access::Write);
        bus.read(0x2007, Access::Read);
        assert_eq!(bus.read(0x2007, Access::Read), 0x66, "chr_ram start");
        bus.write(0x2006, 0x1F, Access::Write);
        bus.write(0x2006, 0xFF, Access::Write);
        bus.read(0x2007, Access::Read);
        assert_eq!(bus.read(0x2007, Access::Read), 0x66, "chr_ram end");

        // Writes allowed
        bus.write(0x2006, 0x10, Access::Write);
        bus.write(0x2006, 0x00, Access::Write);
        bus.write(0x2007, 0x77, Access::Write);

        bus.write(0x2006, 0x10, Access::Write);
        bus.write(0x2006, 0x00, Access::Write);
        bus.read(0x2007, Access::Read);
        assert_eq!(bus.read(0x2007, Access::Read), 0x77, "chr_ram write");
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
        bus.add_genie_code(code.to_string())
            .expect("valid genie code");

        assert_eq!(bus.peek(addr, Access::Read), new_value, "peek code value");
        assert_eq!(bus.read(addr, Access::Read), new_value, "read code value");
        bus.remove_genie_code(code);
        assert_eq!(bus.peek(addr, Access::Read), orig_value, "peek orig value");
        assert_eq!(bus.read(addr, Access::Read), orig_value, "read orig value");
    }

    #[test]
    fn clock() {
        let mut bus = Bus::default();

        bus.clock_to(12);
        assert_eq!(bus.ppu.master_clock(), 12, "ppu clock");
        bus.clock();
        assert_eq!(bus.apu.cycle(), 1, "apu clock");
    }

    #[test]
    fn read_write_ram() {
        let mut bus = Bus::default();

        bus.write(0x0001, 0x66, Access::Write);
        assert_eq!(bus.peek(0x0001, Access::Read), 0x66, "peek ram");
        assert_eq!(bus.read(0x0001, Access::Read), 0x66, "read ram");
        assert_eq!(bus.read(0x0801, Access::Read), 0x66, "peek mirror 1");
        assert_eq!(bus.read(0x0801, Access::Read), 0x66, "read mirror 1");
        assert_eq!(bus.read(0x1001, Access::Read), 0x66, "peek mirror 2");
        assert_eq!(bus.read(0x1001, Access::Read), 0x66, "read mirror 2");
        assert_eq!(bus.read(0x1801, Access::Read), 0x66, "peek mirror 3");
        assert_eq!(bus.read(0x1801, Access::Read), 0x66, "read mirror 3");

        bus.write(0x0802, 0x77, Access::Write);
        assert_eq!(bus.read(0x0002, Access::Read), 0x77, "write mirror 1");
        bus.write(0x1002, 0x88, Access::Write);
        assert_eq!(bus.read(0x0002, Access::Read), 0x88, "write mirror 2");
        bus.write(0x1802, 0x99, Access::Write);
        assert_eq!(bus.read(0x0002, Access::Read), 0x99, "write mirror 3");
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
