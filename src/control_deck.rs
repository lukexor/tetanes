use crate::{
    apu::{Apu, AudioChannel},
    bus::Bus,
    cart::Cart,
    common::{Clocked, NesRegion, Powered},
    cpu::{instr::Instr, Cpu},
    input::{Gamepad, GamepadSlot},
    memory::RamState,
    ppu::{Ppu, VideoFilter},
    NesResult,
};
use anyhow::anyhow;
use std::{io::Read, ops::ControlFlow};

/// Represents an NES Control Deck
#[derive(Debug, Clone)]
#[must_use]
pub struct ControlDeck {
    running: bool,
    ram_state: RamState,
    nes_region: NesRegion,
    loaded_rom: Option<String>,
    turbo_timer: f32,
    cycles_remaining: f32,
    cpu: Cpu,
}

impl ControlDeck {
    /// Creates a new `ControlDeck` instance.
    #[inline]
    pub fn new(nes_region: NesRegion, ram_state: RamState) -> Self {
        let cpu = Cpu::new(nes_region, Bus::new(nes_region, ram_state));
        Self {
            running: false,
            ram_state,
            nes_region,
            loaded_rom: None,
            turbo_timer: 0.0,
            cycles_remaining: 0.0,
            cpu,
        }
    }

    /// Loads a ROM cartridge into memory
    ///
    /// # Errors
    ///
    /// If there is any issue loading the ROM, then an error is returned.
    #[inline]
    pub fn load_rom<S: ToString, F: Read>(&mut self, name: S, rom: &mut F) -> NesResult<()> {
        self.loaded_rom = Some(name.to_string());
        let cart = Cart::from_rom(name, rom, self.ram_state)?;
        self.set_nes_region(cart.nes_region);
        self.cpu.bus.load_cart(cart);
        self.power_cycle();
        Ok(())
    }

    #[inline]
    pub fn load_cpu(&mut self, mut cpu: Cpu) {
        // Swapping CPU swaps Box<Cart>, but we want to maintain the pointer to the original Cart
        self.cpu.bus.cart.swap(&mut cpu.bus.cart);
        std::mem::swap(&mut self.cpu.bus.cart, &mut cpu.bus.cart);
        self.cpu = cpu;
        self.cpu.bus.update_cart();
    }

    #[inline]
    #[must_use]
    pub const fn loaded_rom(&self) -> &Option<String> {
        &self.loaded_rom
    }

    /// Get a frame worth of pixels.
    #[inline]
    #[must_use]
    pub fn frame_buffer(&mut self) -> &[u8] {
        self.cpu.bus.ppu.frame_buffer()
    }

    /// Get the current frame number.
    #[inline]
    #[must_use]
    pub const fn frame_number(&self) -> u32 {
        self.cpu.bus.ppu.frame.num
    }

    /// Get audio samples.
    #[inline]
    #[must_use]
    pub fn audio_samples(&mut self) -> &mut [f32] {
        self.cpu.bus.apu.samples()
    }

    /// Clear audio samples.
    #[inline]
    pub fn clear_audio_samples(&mut self) {
        self.cpu.bus.apu.clear_samples();
    }

    #[inline]
    pub fn clock_rate(&mut self) -> f32 {
        Cpu::clock_rate(self.nes_region)
    }

    /// Steps the control deck one CPU clock.
    ///
    /// # Errors
    ///
    /// If CPU encounteres an invalid opcode, an error is returned.
    pub fn clock_debug(&mut self) -> NesResult<ControlFlow<usize, usize>> {
        let cycles = self.clock();
        if self.cpu_corrupted() {
            Err(anyhow!("cpu corrupted"))
        } else if self.should_break() {
            Ok(ControlFlow::Break(cycles))
        } else {
            Ok(ControlFlow::Continue(cycles))
        }
    }

    /// Steps the control deck the number of seconds.
    ///
    /// # Errors
    ///
    /// If CPU encounteres an invalid opcode, an error is returned.
    #[inline]
    pub fn clock_seconds(&mut self, seconds: f32) -> NesResult<ControlFlow<usize, usize>> {
        self.cycles_remaining += self.clock_rate() * seconds;
        let mut total_cycles = 0;
        while self.cycles_remaining > 0.0 {
            match self.clock_debug()? {
                ControlFlow::Break(cycles) => {
                    total_cycles += cycles;
                    self.cycles_remaining -= cycles as f32;
                    return Ok(ControlFlow::Break(total_cycles));
                }
                ControlFlow::Continue(cycles) => {
                    total_cycles += cycles;
                    self.cycles_remaining -= cycles as f32;
                }
            }
        }
        Ok(ControlFlow::Continue(total_cycles))
    }

    /// Steps the control deck an entire frame
    ///
    /// # Errors
    ///
    /// If CPU encounteres an invalid opcode, an error is returned.
    #[inline]
    pub fn clock_frame(&mut self) -> NesResult<ControlFlow<usize, usize>> {
        let mut total_cycles = 0;
        let frame = self.frame_number();
        while frame == self.frame_number() {
            match self.clock_debug()? {
                ControlFlow::Break(cycles) => {
                    total_cycles += cycles;
                    self.cycles_remaining -= cycles as f32;
                    return Ok(ControlFlow::Break(total_cycles));
                }
                ControlFlow::Continue(cycles) => {
                    total_cycles += cycles;
                    self.cycles_remaining -= cycles as f32;
                }
            }
        }
        Ok(ControlFlow::Continue(total_cycles))
    }

    /// Steps the control deck a single scanline.
    ///
    /// # Errors
    ///
    /// If CPU encounteres an invalid opcode, an error is returned.
    #[inline]
    pub fn clock_scanline(&mut self) -> NesResult<ControlFlow<usize, usize>> {
        let current_scanline = self.cpu.bus.ppu.scanline;
        let mut total_cycles = 0;
        while current_scanline == self.cpu.bus.ppu.scanline {
            match self.clock_debug()? {
                ControlFlow::Break(cycles) => {
                    total_cycles += cycles;
                    self.cycles_remaining -= cycles as f32;
                    return Ok(ControlFlow::Break(total_cycles));
                }
                ControlFlow::Continue(cycles) => {
                    total_cycles += cycles;
                    self.cycles_remaining -= cycles as f32;
                }
            }
        }
        Ok(ControlFlow::Continue(total_cycles))
    }

    /// Returns whether the CPU is corrupted or not.
    #[inline]
    #[must_use]
    pub const fn cpu_corrupted(&self) -> bool {
        self.cpu.corrupted
    }

    /// Returns whether the CPU debugger should break or not.
    #[inline]
    #[must_use]
    pub const fn should_break(&self) -> bool {
        // TODO
        false
    }

    /// Returns the current CPU program counter.
    #[inline]
    #[must_use]
    pub const fn pc(&self) -> u16 {
        self.cpu.pc
    }

    /// Returns the next CPU instruction to be executed.
    #[inline]
    pub fn next_instr(&self) -> Instr {
        self.cpu.next_instr()
    }

    /// Returns the next address on the bus with the current value at the target address, if
    /// appropriate.
    #[inline]
    #[must_use]
    pub fn next_addr(&self) -> (Option<u16>, Option<u16>) {
        self.cpu.next_addr()
    }

    /// Returns the address at the top of the stack.
    #[inline]
    #[must_use]
    pub fn stack_addr(&self) -> u16 {
        self.cpu.peek_stack_word()
    }

    /// Disassemble an address range of CPU instructions.
    #[inline]
    #[must_use]
    pub fn disasm(&self, start: u16, end: u16) -> Vec<String> {
        let mut disassembly = Vec::with_capacity(256);
        let mut addr = start;
        while addr <= end {
            disassembly.push(self.cpu.disassemble(&mut addr));
        }
        disassembly
    }

    #[inline]
    pub const fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    #[inline]
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    #[inline]
    pub const fn ppu(&self) -> &Ppu {
        &self.cpu.bus.ppu
    }

    #[inline]
    pub fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.cpu.bus.ppu
    }

    #[inline]
    pub const fn apu(&self) -> &Apu {
        &self.cpu.bus.apu
    }

    #[inline]
    pub const fn cart(&self) -> &Cart {
        &self.cpu.bus.cart
    }

    #[inline]
    pub fn cart_mut(&mut self) -> &mut Cart {
        &mut self.cpu.bus.cart
    }

    /// Returns whether Four Score is enabled.
    #[inline]
    #[must_use]
    pub const fn fourscore(&self) -> bool {
        self.cpu.bus.input.fourscore
    }

    /// Enable Four Score.
    #[inline]
    pub fn set_fourscore(&mut self, enabled: bool) {
        self.cpu.bus.input.fourscore = enabled;
    }

    /// Returns a mutable reference to a gamepad.
    #[inline]
    pub fn gamepad_mut(&mut self, slot: GamepadSlot) -> &mut Gamepad {
        &mut self.cpu.bus.input.gamepads[slot as usize]
    }

    /// Returns the zapper aiming position for the given controller slot.
    #[inline]
    #[must_use]
    pub const fn zapper_pos(&self, slot: GamepadSlot) -> (i32, i32) {
        let zapper = self.cpu.bus.input.zappers[slot as usize];
        (zapper.x, zapper.y)
    }

    /// Returns whether zapper gun is connected to a given controller slot.
    #[inline]
    #[must_use]
    pub const fn zapper_connected(&self, slot: GamepadSlot) -> bool {
        self.cpu.bus.input.zappers[slot as usize].connected
    }

    /// Connect Zapper gun to a given controller slot.
    #[inline]
    pub fn connect_zapper(&mut self, slot: GamepadSlot, connected: bool) {
        self.cpu.bus.input.zappers[slot as usize].connected = connected;
    }

    /// Trigger Zapper gun for a given controller slot.
    #[inline]
    pub fn trigger_zapper(&mut self, slot: GamepadSlot) {
        self.cpu.bus.input.zappers[slot as usize].trigger();
    }

    /// Aim Zapper gun for a given controller slot.
    #[inline]
    pub fn aim_zapper(&mut self, slot: GamepadSlot, x: i32, y: i32) {
        let zapper = &mut self.cpu.bus.input.zappers[slot as usize];
        zapper.x = x;
        zapper.y = y;
    }

    /// Get the NES format for the emulation.
    #[inline]
    pub fn nes_region(&mut self) -> NesRegion {
        self.nes_region
    }

    /// Set the NES format for the emulation.
    #[inline]
    pub fn set_nes_region(&mut self, nes_region: NesRegion) {
        self.nes_region = nes_region;
        self.cpu.set_nes_region(nes_region);
    }

    /// Get the video filter for the emulation.
    #[inline]
    pub const fn filter(&self) -> VideoFilter {
        self.cpu.bus.ppu.filter
    }

    /// Set the video filter for the emulation.
    #[inline]
    pub fn set_filter(&mut self, filter: VideoFilter) {
        self.cpu.bus.ppu.filter = filter;
    }

    /// Returns whether a given API audio channel is enabled.
    #[inline]
    pub fn channel_enabled(&mut self, channel: AudioChannel) -> bool {
        self.cpu.bus.apu.channel_enabled(channel)
    }

    /// Toggle one of the APU audio channels.
    #[inline]
    pub fn toggle_channel(&mut self, channel: AudioChannel) {
        self.cpu.bus.apu.toggle_channel(channel);
    }

    /// Is control deck running.
    #[inline]
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }
}

impl Default for ControlDeck {
    fn default() -> Self {
        Self::new(NesRegion::default(), RamState::default())
    }
}

impl Clocked for ControlDeck {
    /// Steps the control deck a single clock cycle.
    #[inline]
    fn clock(&mut self) -> usize {
        for zapper in &mut self.cpu.bus.input.zappers {
            zapper.clock();
        }
        self.turbo_timer -= 1.0;
        if self.turbo_timer <= 0.0 {
            self.turbo_timer += self.clock_rate() / 30.0;
            for gamepad in &mut self.cpu.bus.input.gamepads {
                if gamepad.turbo_a {
                    gamepad.a = !gamepad.a;
                }
                if gamepad.turbo_b {
                    gamepad.b = !gamepad.b;
                }
            }
        }
        self.cpu.clock()
    }
}

impl Powered for ControlDeck {
    /// Powers on the console
    #[inline]
    fn power_on(&mut self) {
        self.cpu.power_on();
        self.running = true;
    }

    /// Powers off the console
    #[inline]
    fn power_off(&mut self) {
        self.cpu.power_off();
        self.running = false;
    }

    /// Soft-resets the console
    #[inline]
    fn reset(&mut self) {
        self.cpu.reset();
        self.running = true;
    }

    /// Hard-resets the console
    #[inline]
    fn power_cycle(&mut self) {
        self.cpu.power_cycle();
        self.running = true;
    }
}
