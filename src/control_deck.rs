use crate::{
    apu::{Apu, AudioChannel},
    bus::Bus,
    cart::Cart,
    common::{Clocked, NesFormat, Powered},
    cpu::{instr::Instr, Cpu, CPU_CLOCK_RATE},
    debugger::Breakpoint,
    input::{Gamepad, GamepadSlot},
    memory::RamState,
    ppu::{Ppu, VideoFilter},
    NesResult,
};
use std::{io::Read, ops::ControlFlow};

/// Represents an NES Control Deck
#[derive(Debug, Clone)]
#[must_use]
pub struct ControlDeck {
    running: bool,
    ram_state: RamState,
    loaded_rom: Option<String>,
    turbo_clock: usize,
    cycles_remaining: f32,
    cpu: Cpu,
}

impl ControlDeck {
    /// Creates a new `ControlDeck` instance.
    #[inline]
    pub fn new(nes_format: NesFormat, ram_state: RamState) -> Self {
        let cpu = Cpu::init(nes_format, Bus::new(nes_format, ram_state));
        Self {
            running: false,
            ram_state,
            loaded_rom: None,
            turbo_clock: 0,
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
    pub fn load_rom<S: ToString, F: Read>(&mut self, name: &S, rom: &mut F) -> NesResult<()> {
        self.loaded_rom = Some(name.to_string());
        let cart = Cart::from_rom(name, rom, self.ram_state)?;
        self.cpu.bus.load_cart(cart);
        self.power_cycle();
        Ok(())
    }

    #[inline]
    pub fn load_cpu(&mut self, mut cpu: Cpu) {
        // Swap CPU, but keep original loaded cart, except for ram and mapper
        std::mem::swap(&mut self.cpu, &mut cpu);
        std::mem::swap(&mut self.cpu.bus.cart, &mut cpu.bus.cart);
        self.cpu.bus.ppu.load_cart(&mut self.cpu.bus.cart);
        self.cpu.bus.apu.load_cart(&mut self.cpu.bus.cart);
        self.cpu.bus.cart.prg_ram = cpu.bus.cart.prg_ram;
        self.cpu.bus.cart.chr = cpu.bus.cart.chr;
        self.cpu.bus.cart.mapper = cpu.bus.cart.mapper;
    }

    #[inline]
    #[must_use]
    pub const fn loaded_rom(&self) -> &Option<String> {
        &self.loaded_rom
    }

    /// Get a frame worth of pixels.
    #[inline]
    #[must_use]
    pub fn frame_buffer(&self) -> &[u8] {
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
    pub fn audio_samples(&self) -> &[f32] {
        self.cpu.bus.apu.samples()
    }

    /// Clear audio samples.
    #[inline]
    pub fn clear_audio_samples(&mut self) {
        self.cpu.bus.apu.clear_samples();
    }

    /// Set the emulation speed.
    #[inline]
    pub fn set_speed(&mut self, speed: f32) {
        self.cpu.bus.apu.set_speed(speed);
    }

    /// Steps the control deck the number of seconds
    #[inline]
    pub fn clock_seconds(&mut self, seconds: f32) -> usize {
        self.cycles_remaining += CPU_CLOCK_RATE * seconds;
        let mut clocks = 0;
        while self.cycles_remaining > 0.0 && !self.cpu_corrupted() {
            let cycles = self.clock();
            clocks += cycles;
            self.cycles_remaining -= cycles as f32;
        }
        clocks
    }

    #[inline]
    pub(crate) fn debug_clock_frame(
        &mut self,
        breakpoints: &[Breakpoint],
    ) -> ControlFlow<usize, usize> {
        self.clock_input();
        let mut clocks = 0;
        while !self.frame_complete() && !self.cpu_corrupted() {
            if breakpoints.iter().any(|bp| bp.matches(&self.cpu)) {
                return ControlFlow::Break(clocks);
            }
            clocks += self.clock();
        }
        self.start_new_frame();
        ControlFlow::Continue(clocks)
    }

    /// Steps the control deck an entire frame
    #[inline]
    pub fn clock_frame(&mut self) -> usize {
        self.clock_input();
        let mut clocks = 0;
        while !self.frame_complete() && !self.cpu_corrupted() {
            clocks += self.clock();
        }
        self.start_new_frame();
        clocks
    }

    /// Steps the control deck a single scanline.
    #[inline]
    pub fn clock_scanline(&mut self) -> usize {
        let current_scanline = self.cpu.bus.ppu.scanline;
        let mut clocks = 0;
        while self.cpu.bus.ppu.scanline == current_scanline && !self.cpu_corrupted() {
            clocks += self.clock();
        }
        clocks
    }

    /// Returns whether the CPU is corrupted or not.
    #[inline]
    #[must_use]
    pub const fn cpu_corrupted(&self) -> bool {
        self.cpu.corrupted
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

    #[inline]
    #[must_use]
    pub const fn frame_complete(&self) -> bool {
        self.cpu.bus.ppu.frame_complete
    }

    #[inline]
    pub fn start_new_frame(&mut self) {
        self.cpu.bus.ppu.frame_complete = false;
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

impl ControlDeck {
    #[inline]
    fn clock_input(&mut self) {
        for zapper in &mut self.cpu.bus.input.zappers {
            zapper.update();
        }
        self.turbo_clock += 1;
        // Every 2 frames, ~30Hz turbo
        if self.turbo_clock > 2 {
            self.turbo_clock = 0;
        }
        let turbo = self.turbo_clock == 0;
        for gamepad in &mut self.cpu.bus.input.gamepads {
            if gamepad.turbo_a {
                gamepad.a = turbo;
            }
            if gamepad.turbo_b {
                gamepad.b = turbo;
            }
        }
    }
}

impl Default for ControlDeck {
    fn default() -> Self {
        Self::new(NesFormat::default(), RamState::default())
    }
}

impl Clocked for ControlDeck {
    /// Steps the control deck a single clock cycle.
    #[inline]
    fn clock(&mut self) -> usize {
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
