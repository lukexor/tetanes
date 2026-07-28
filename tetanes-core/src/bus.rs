//! NES Memory/Data Bus implementation.
//!
//! <https://wiki.nesdev.org/w/index.php/CPU_memory_map>
//!
//! # Stability
//!
//! [`Bus`]'s fields are the emulation's internal wiring - the components it routes to, and the
//! open-bus and region state it routes with. They are public so that embedders and debuggers can
//! reach the component tree, but they track the implementation rather than the crate version, and
//! a release may add, rename or retype any of them. The stable entry point is
//! [`ControlDeck`](crate::control_deck::ControlDeck).

use crate::{
    apu::{Apu, Channel},
    cart::Cart,
    common::{NesRegion, ResetKind},
    fs,
    genie::GenieCode,
    input::{Input, Player},
    mapper::{Mapper, MapperOps},
    memory::{ConstArray, RamState, Read, Write},
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
    /// 2K of work RAM on the console itself, at $0000-$07FF and mirrored to $1FFF.
    //
    // Measured un-boxed (embedded directly in `Bus`): ~1.2% slower on the bench corpus, not
    // faster, despite removing a pointer chase - inlining it grows `Bus`'s footprint enough to
    // outweigh that. Keep it boxed.
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
    //! Memory size constants.

    /// 2K of work RAM on the console itself, available to the CPU.
    pub const WRAM: usize = 0x800;
}

impl Bus {
    /// Creates a bus timed for `region`, with work RAM initialised per `ram_state`.
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

    /// Installs a cart, handing its board and memory to the PPU.
    pub fn load_cart(&mut self, cart: Cart) {
        self.ppu.load_cart(cart.mapper, cart.memory);
    }

    /// Removes the cart, leaving the console with no board.
    pub fn unload_cart(&mut self) {
        self.ppu.load_mapper(Mapper::default());
    }

    /// The console's 2K of work RAM.
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
        // This runs on every PRG read, so skip hashing the address entirely in the overwhelmingly
        // common case of no codes being loaded.
        if self.genie_codes.is_empty() {
            return val;
        }
        self.genie_codes
            .get(&addr)
            .map_or(val, |genie_code| genie_code.read(val))
    }

    /// Samples the APU has mixed since the last clear.
    #[inline]
    #[must_use]
    pub fn audio_samples(&self) -> &[f32] {
        &self.apu.audio_samples
    }

    /// Drops the mixed samples, which the clocking API does at the start of each call.
    #[inline]
    pub fn clear_audio_samples(&mut self) {
        self.apu.audio_samples.clear();
    }

    /// Clocks everything the CPU's cycle drives: the board, the APU, input and the PPU.
    #[inline(always)]
    pub fn cpu_clock(&mut self) {
        let ops = self.ppu.mapper_ops;
        if ops.intersects(MapperOps::CLOCKED) {
            self.ppu.mapper.clock();
        }
        let output = if ops.intersects(MapperOps::AUDIO) {
            self.ppu.mapper.output()
        } else {
            0.0
        };
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
                let val = self
                    .ppu
                    .mapper_ops
                    .intersects(MapperOps::SERVES_PRG_READS)
                    .then(|| self.ppu.mapper.prg_read(addr))
                    .flatten()
                    .unwrap_or_else(|| self.ppu.memory.prg_peek(addr));
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
                let val = self
                    .ppu
                    .mapper_ops
                    .intersects(MapperOps::SERVES_PRG_READS)
                    .then(|| self.ppu.mapper.prg_peek(addr))
                    .flatten()
                    .unwrap_or_else(|| self.ppu.memory.prg_peek(addr));
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
            0x4100..=0xFFFF => {
                // Data store first, then let the board act on any register the write hit.
                // Destructured so both fields can be borrowed at once.
                let Ppu { mapper, memory, .. } = &mut self.ppu;
                memory.prg_write(addr, val);
                mapper.write_register(memory, addr, val);
            }
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

impl Bus {
    /// The region the bus and its components are timed for.
    pub const fn region(&self) -> NesRegion {
        self.region
    }

    /// Sets the region, forwarding it to every component.
    pub fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.ppu.set_region(region);
        self.apu.set_region(region);
        self.input.set_region(region);
    }

    /// Resets the bus and every component. A hard reset also re-initialises work RAM.
    pub fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            self.ram_state.fill(&mut **self.wram);
        }
        self.ppu.reset(kind);
        self.apu.reset(kind);
    }

    /// Writes battery-backed cart RAM to `path`.
    ///
    /// # Errors
    ///
    /// If the file cannot be written.
    pub fn save(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        self.ppu.mapper.save_sram(&self.ppu.memory, path.as_ref())
    }

    /// Reads battery-backed cart RAM from `path`.
    ///
    /// # Errors
    ///
    /// If the file cannot be read.
    pub fn load(&mut self, path: impl AsRef<Path>) -> fs::Result<()> {
        let Ppu { mapper, memory, .. } = &mut self.ppu;
        mapper.load_sram(memory, path.as_ref())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        apu::noise::ShiftMode,
        input::JoypadBtn,
        mapper::{Cnrom, Nrom},
        memory::Src,
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
        // Cnrom doesn't provide CHR-RAM
        let mut cart = Cart::empty_sized(0x4000, 0x2000);
        cart.mapper = Cnrom::load(&mut cart).unwrap();
        cart.memory.region_mut(Src::Chr).fill(0x66);
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
        // A zero-sized CHR-ROM yields CHR-RAM.
        let mut cart = Cart::empty_sized(0x4000, 0);
        cart.mapper = Nrom::load(&mut cart).unwrap();
        cart.memory.region_mut(Src::Chr).fill(0x66);
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
        let mut cart = Cart::empty_sized(0x8000, 0x2000);

        let code = "YYKPOYZZ"; // The Legend of Zelda: New character with 8 Hearts
        let addr = 0x9F41;
        let orig_value = 0x22; // 3 Hearts
        let new_value = 0x77; // 8 Hearts

        cart.mapper = Nrom::load(&mut cart).unwrap();
        cart.memory.region_mut(Src::PrgRom)[(addr & 0x7FFF) as usize] = orig_value;

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

    /// $2000-$2007 repeat every 8 bytes up to $3FFF, so the mirror mask is what decides which
    /// register a write lands on. Reads of the write-only registers return the PPU's open bus
    /// rather than anything latched.
    #[test]
    fn read_write_ppu() {
        let mut bus = Bus::default();

        bus.write(0x2000, 0x80);
        assert_eq!(bus.ppu.ctrl.bits.bits(), 0x80, "$2000 PPUCTRL");
        bus.write(0x3FF8, 0x00);
        assert_eq!(bus.ppu.ctrl.bits.bits(), 0x00, "$3FF8 mirrors $2000");

        bus.write(0x2001, 0x1E);
        assert_eq!(bus.ppu.mask.bits.bits(), 0x1E, "$2001 PPUMASK");
        bus.write(0x3FF9, 0x00);
        assert_eq!(bus.ppu.mask.bits.bits(), 0x00, "$3FF9 mirrors $2001");
        bus.write(0x2003, 0x42);
        assert_eq!(bus.ppu.oamaddr, 0x42, "$2003 OAMADDR");

        // OAMDATA round-trips through $2004, and the address post-increments on write.
        bus.write(0x2003, 0x10);
        bus.write(0x2004, 0x99);
        assert_eq!(bus.ppu.oamaddr, 0x11, "OAMADDR increments on write");
        bus.write(0x2003, 0x10);
        assert_eq!(bus.read(0x2004), 0x99, "$2004 OAMDATA");

        // The write-only registers read back the PPU's open bus, not their contents.
        bus.ppu.open_bus = 0xA5;
        for addr in [0x2000, 0x2001, 0x2003, 0x2005, 0x2006] {
            assert_eq!(bus.read(addr), 0xA5, "${addr:04X} is write-only");
        }

        // $2002 clears the vblank flag as a side effect, so a second read differs - and `peek`
        // must not do it.
        bus.ppu.status.set_in_vblank(true);
        assert_ne!(bus.peek(0x2002) & 0x80, 0, "peek sees vblank");
        assert_ne!(bus.peek(0x2002) & 0x80, 0, "and leaves it set");
        assert_ne!(bus.read(0x2002) & 0x80, 0, "read sees vblank");
        assert_eq!(bus.read(0x2002) & 0x80, 0, "and clears it");
    }

    /// $4015 is the APU status register both ways, but $4017 is not symmetric: writing it sets the
    /// APU frame counter while reading it returns controller two. Getting that backwards is silent.
    #[test]
    fn read_write_apu() {
        let mut bus = Bus::default();

        // $4015 write enables length counters; reading it reports which are non-zero.
        bus.write(0x4015, 0x0F);
        assert!(bus.apu.pulse1.length.enabled, "pulse1 enabled");
        assert!(bus.apu.pulse2.length.enabled, "pulse2 enabled");
        assert!(bus.apu.triangle.length.enabled, "triangle enabled");
        assert!(bus.apu.noise.length.enabled, "noise enabled");
        bus.write(0x4015, 0x00);
        assert!(!bus.apu.pulse1.length.enabled, "pulse1 disabled");

        // $4017 write is the frame counter; bit 6 inhibits the frame IRQ.
        bus.write(0x4017, 0x40);
        assert!(
            bus.apu.frame_counter.inhibit_irq,
            "$4017 bit 6 inhibits the frame IRQ"
        );

        // $4017 read is controller two, not the frame counter.
        bus.input.joypads[1].set_button(JoypadBtn::A, true);
        bus.write(0x4016, 0x01);
        bus.write(0x4016, 0x00);
        assert_eq!(bus.read(0x4017) & 0x01, 0x01, "$4017 reads controller two");
    }

    /// $4000-$4003 is pulse 1 and $4004-$4007 is pulse 2. The two blocks must stay independent.
    #[test]
    fn write_apu_pulse() {
        let mut bus = Bus::default();

        bus.write(0x4000, 0x3F); // duty 0, constant volume 15
        bus.write(0x4002, 0x34); // timer low
        bus.write(0x4003, 0x01); // timer high
        assert_eq!(bus.apu.pulse1.real_period, 0x134, "pulse1 period");
        assert_eq!(bus.apu.pulse2.real_period, 0, "pulse2 untouched");

        bus.write(0x4006, 0x78);
        bus.write(0x4007, 0x02);
        assert_eq!(bus.apu.pulse2.real_period, 0x278, "pulse2 period");
        assert_eq!(bus.apu.pulse1.real_period, 0x134, "pulse1 still untouched");

        // $4001/$4005 are the sweep units.
        bus.write(0x4001, 0x8F);
        assert!(bus.apu.pulse1.sweep.enabled, "$4001 pulse1 sweep");
        assert!(!bus.apu.pulse2.sweep.enabled, "pulse2 sweep untouched");
        bus.write(0x4005, 0x8F);
        assert!(bus.apu.pulse2.sweep.enabled, "$4005 pulse2 sweep");
    }

    /// $4008/$400A/$400B is the triangle. $4009 is unmapped and must do nothing.
    #[test]
    fn write_apu_triangle() {
        let mut bus = Bus::default();

        bus.write(0x400A, 0x56);
        bus.write(0x400B, 0x03);
        assert_eq!(bus.apu.triangle.timer.period, 0x356, "triangle period");

        bus.write(0x4008, 0x7F);
        assert_eq!(
            bus.apu.triangle.linear.counter_reload, 0x7F,
            "$4008 linear counter"
        );

        // $4009 is not a register; it must not disturb the channel.
        let before = bus.apu.triangle.timer.period;
        bus.write(0x4009, 0xFF);
        assert_eq!(bus.apu.triangle.timer.period, before, "$4009 is unmapped");
    }

    /// $400C/$400E/$400F is the noise channel. $400D is unmapped.
    #[test]
    fn write_apu_noise() {
        let mut bus = Bus::default();

        bus.write(0x400E, 0x80 | 0x04);
        assert_eq!(bus.apu.noise.shift_mode, ShiftMode::One, "$400E shift mode");

        bus.write(0x400C, 0x3F);
        assert!(bus.apu.noise.envelope.constant_volume, "$400C envelope");

        // The length counter latches a reload value here; the frame counter loads it later.
        bus.write(0x4015, 0x08); // enable, or the write is ignored entirely
        bus.write(0x400F, 0x08);
        assert_ne!(bus.apu.noise.length.reload, 0, "$400F length reload");
        bus.write(0x4015, 0x00);
        bus.write(0x400F, 0x10);
        assert_eq!(
            bus.apu.noise.length.reload, 254,
            "a disabled channel ignores the write"
        );

        let before = bus.apu.noise.timer.period;
        bus.write(0x400D, 0xFF);
        assert_eq!(bus.apu.noise.timer.period, before, "$400D is unmapped");
    }

    /// $4010-$4013 is the DMC. Sample address and length are stored scaled, not raw.
    #[test]
    fn write_dmc() {
        let mut bus = Bus::default();

        bus.write(0x4010, 0x0F); // rate index 15, IRQ and loop clear
        assert!(!bus.apu.dmc.irq_enabled, "$4010 IRQ disabled");
        assert!(!bus.apu.dmc.loops, "$4010 loop clear");
        bus.write(0x4010, 0xC0);
        assert!(bus.apu.dmc.irq_enabled, "$4010 bit 7 enables the IRQ");
        assert!(bus.apu.dmc.loops, "$4010 bit 6 sets loop");

        bus.write(0x4011, 0xFF);
        assert_eq!(bus.apu.dmc.output_level, 0x7F, "$4011 keeps 7 bits");

        bus.write(0x4012, 0x02);
        assert_eq!(bus.apu.dmc.sample_addr, 0xC080, "$4012 is $C000 + n*64");

        bus.write(0x4013, 0x02);
        assert_eq!(bus.apu.dmc.sample_length, 0x21, "$4013 is n*16 + 1");
    }

    /// $4016 writes strobe both controllers; $4016 and $4017 read them back one bit at a time.
    #[test]
    fn read_write_input() {
        let mut bus = Bus::default();

        bus.input.joypads[0].set_button(JoypadBtn::A, true);
        bus.input.joypads[0].set_button(JoypadBtn::Right, true);

        // Strobe high then low latches the button state and rewinds to bit 0.
        bus.write(0x4016, 0x01);
        bus.write(0x4016, 0x00);

        // A, B, Select, Start, Up, Down, Left, Right - so bit 0 is A and bit 7 is Right.
        let bits: Vec<u8> = (0..8).map(|_| bus.read(0x4016) & 0x01).collect();
        assert_eq!(bits, [1, 0, 0, 0, 0, 0, 0, 1], "controller one shifts out");

        // Re-strobing rewinds it.
        bus.write(0x4016, 0x01);
        bus.write(0x4016, 0x00);
        assert_eq!(bus.read(0x4016) & 0x01, 0x01, "back to the A button");

        // Controller two is a separate shift register on $4017.
        assert_eq!(bus.read(0x4017) & 0x01, 0x00, "controller two is idle");
    }

    /// Everything from $4100 up is the cartridge: the write goes to memory first and then to the
    /// board, and reads come back through the page table.
    #[test]
    fn read_write_mapper() {
        let mut bus = Bus::default();
        // 16K of CHR-ROM, i.e. two 8K banks, so a bank switch is visible.
        let mut cart = Cart::empty_sized(0x8000, 0x4000);
        cart.mapper = Cnrom::load(&mut cart).expect("valid mapper");
        cart.memory.region_mut(Src::Chr).fill(0x11);
        cart.memory.region_mut(Src::Chr)[0x2000..].fill(0x22);
        bus.load_cart(cart);

        let read_chr = |bus: &mut Bus| {
            bus.write(0x2006, 0x00);
            bus.write(0x2006, 0x00);
            bus.read(0x2007); // discard the buffered read
            bus.read(0x2007)
        };
        assert_eq!(read_chr(&mut bus), 0x11, "CHR bank 0");

        // CNROM takes its bank from any write to $8000-$FFFF.
        bus.write(0x8000, 0x01);
        assert_eq!(read_chr(&mut bus), 0x22, "the write reached the board");

        // Below $4100 is not the cartridge, so it must not reach the board.
        bus.write(0x4000, 0x00);
        assert_eq!(read_chr(&mut bus), 0x22, "$4000 is the APU, not the mapper");
    }

    /// A hard reset re-fills WRAM from the configured RAM state; a soft reset leaves it alone.
    #[test]
    fn reset() {
        let mut bus = Bus {
            ram_state: RamState::AllZeros,
            ..Default::default()
        };

        bus.write(0x0001, 0x66);
        bus.write(0x2000, 0x80);

        bus.reset(ResetKind::Soft);
        assert_eq!(bus.peek(0x0001), 0x66, "a soft reset preserves WRAM");

        bus.reset(ResetKind::Hard);
        assert_eq!(bus.peek(0x0001), 0x00, "a hard reset clears WRAM");
    }
}
