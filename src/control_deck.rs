use crate::{
    apu::{Apu, AudioChannel},
    bus::Bus,
    cart::Cart,
    common::{Clocked, Powered},
    cpu::{instr::Instr, Cpu, CPU_CLOCK_RATE},
    input::{Gamepad, GamepadSlot},
    memory::{MemAccess, RamState},
    ppu::{Ppu, VideoFormat},
    NesResult,
};
use std::io::Read;

/// Represents an NES Control Deck
#[derive(Debug, Clone)]
#[must_use]
pub struct ControlDeck {
    running: bool,
    power_state: RamState,
    loaded_rom: Option<String>,
    turbo_clock: usize,
    cycles_remaining: f32,
    cpu: Cpu,
}

impl ControlDeck {
    /// Creates a new `ControlDeck` instance.
    pub fn new(power_state: RamState) -> Self {
        let cpu = Cpu::init(Bus::new(power_state));
        Self {
            running: false,
            power_state,
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
    pub fn load_rom<F: Read>(&mut self, name: &str, rom: &mut F) -> NesResult<()> {
        self.power_off();
        self.loaded_rom = Some(name.to_owned());
        let cart = Cart::from_rom(name, rom, self.power_state)?;
        self.cpu.bus.load_cart(cart);
        self.power_on();
        Ok(())
    }

    /// Get a frame worth of pixels
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        self.cpu.bus.ppu.frame()
    }

    /// Get audio samples.
    #[must_use]
    pub fn audio_samples(&self) -> &[f32] {
        self.cpu.bus.apu.samples()
    }

    /// Clear audio samples.
    pub fn clear_audio_samples(&mut self) {
        self.cpu.bus.apu.clear_samples();
    }

    /// Set the emulation speed.
    pub fn set_speed(&mut self, speed: f32) {
        self.cpu.bus.apu.set_speed(speed);
    }

    /// Steps the control deck the number of seconds
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

    /// Steps the control deck an entire frame
    pub fn clock_frame(&mut self) -> usize {
        self.clock_turbo();
        let mut clocks = 0;
        while !self.frame_complete() && !self.cpu_corrupted() {
            clocks += self.clock();
        }
        self.start_new_frame();
        clocks
    }

    /// Steps the control deck a single scanline.
    pub fn clock_scanline(&mut self) -> usize {
        let current_scanline = self.cpu.bus.ppu.scanline;
        let mut clocks = 0;
        while self.cpu.bus.ppu.scanline == current_scanline && !self.cpu_corrupted() {
            clocks += self.clock();
        }
        clocks
    }

    /// Returns whether the CPU is corrupted or not.
    pub fn cpu_corrupted(&self) -> bool {
        self.cpu.corrupted
    }

    /// Returns the current CPU program counter.
    pub fn pc(&self) -> u16 {
        self.cpu.pc
    }

    /// Returns the next CPU instruction to be executed.
    pub fn next_instr(&self) -> Instr {
        self.cpu.next_instr()
    }

    /// Returns the next address on the bus to be either read or written to along with the current
    /// value at the target address.
    pub fn next_addr(&self, access: MemAccess) -> (Option<u16>, Option<u16>) {
        self.cpu.next_addr(access)
    }

    /// Returns the address at the top of the stack.
    pub fn stack_addr(&self) -> u16 {
        self.cpu.peek_stackw()
    }

    /// Disassemble an address range of CPU instructions.
    pub fn disasm(&self, start: u16, end: u16) -> Vec<String> {
        let mut disassembly = Vec::with_capacity(256);
        let mut addr = start;
        while addr <= end {
            disassembly.push(self.cpu.disassemble(&mut addr));
        }
        disassembly
    }

    pub fn set_debug_scanline(&mut self, scanline: u16) {
        self.cpu.bus.ppu.set_debug_scanline(scanline);
    }

    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    pub fn ppu(&self) -> &Ppu {
        &self.cpu.bus.ppu
    }

    pub fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.cpu.bus.ppu
    }

    pub fn apu(&self) -> &Apu {
        &self.cpu.bus.apu
    }

    pub fn cart(&self) -> &Cart {
        &self.cpu.bus.cart
    }

    pub fn apu_info(&self) {
        log::info!("DMC Period: {}", self.cpu.bus.apu.dmc.freq_timer);
        log::info!("DMC Timer: {}", self.cpu.bus.apu.dmc.freq_counter);
        log::info!("DMC Sample Address: 0x{:04X}", self.cpu.bus.apu.dmc.addr);
        log::info!("DMC Sample Length: {}", self.cpu.bus.apu.dmc.length_load);
        log::info!("DMC Bytes Remaining: {}", self.cpu.bus.apu.dmc.output_bits);
    }

    pub fn frame_complete(&self) -> bool {
        self.cpu.bus.ppu.frame_complete
    }

    pub fn start_new_frame(&mut self) {
        self.cpu.bus.ppu.frame_complete = false;
    }

    /// Returns a mutable reference to a gamepad.
    pub fn get_gamepad_mut(&mut self, gamepad: GamepadSlot) -> &mut Gamepad {
        &mut self.cpu.bus.input.gamepads[gamepad as usize]
    }

    /// Get the video filter for the emulation.
    pub const fn filter(&self) -> VideoFormat {
        self.cpu.bus.ppu.filter
    }

    /// Set the video filter for the emulation.
    pub fn set_filter(&mut self, filter: VideoFormat) {
        self.cpu.bus.ppu.filter = filter;
    }

    /// Returns whether a given API audio channel is enabled.
    pub fn channel_enabled(&mut self, channel: AudioChannel) -> bool {
        self.cpu.bus.apu.channel_enabled(channel)
    }

    /// Toggle one of the APU audio channels.
    pub fn toggle_channel(&mut self, channel: AudioChannel) {
        self.cpu.bus.apu.toggle_channel(channel);
    }

    /// Is control deck running.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }
}

impl ControlDeck {
    fn clock_turbo(&mut self) {
        self.turbo_clock += 1;
        if self.turbo_clock > 3 {
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
        Self::new(RamState::default())
    }
}

impl Clocked for ControlDeck {
    /// Steps the control deck a single clock cycle.
    fn clock(&mut self) -> usize {
        self.cpu.clock()
    }
}

impl Powered for ControlDeck {
    /// Powers on the console
    fn power_on(&mut self) {
        self.cpu.power_on();
        self.running = true;
    }

    /// Powers off the console
    fn power_off(&mut self) {
        self.cpu.power_cycle();
        self.cpu.power_off();
        self.running = false;
    }

    /// Soft-resets the console
    fn reset(&mut self) {
        self.cpu.reset();
        self.running = true;
    }

    /// Hard-resets the console
    fn power_cycle(&mut self) {
        self.cpu.power_cycle();
        self.running = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MemRead, MemWrite};
    use std::{fs::File, io::BufReader, path::PathBuf};

    fn load(file: &str) -> ControlDeck {
        let mut deck = ControlDeck::new(RamState::AllZeros);
        let rom = File::open(PathBuf::from(file)).unwrap();
        let mut rom = BufReader::new(rom);
        deck.load_rom(file, &mut rom).unwrap();
        deck.power_on();
        deck
    }

    #[test]
    fn nestest() {
        let rom = "test_roms/cpu/nestest.nes";
        let mut deck = load(rom);
        deck.cpu.pc = 0xC000; // Start automated tests
        deck.clock_seconds(1.0);
        assert_eq!(deck.cpu.peek(0x0000), 0x00, "{}", rom);
    }

    #[test]
    fn dummy_writes_oam() {
        let rom = "test_roms/cpu/dummy_writes_oam.nes";
        let mut deck = load(rom);
        deck.clock_seconds(6.0);
        assert_eq!(deck.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn dummy_writes_ppumem() {
        let rom = "test_roms/cpu/dummy_writes_ppumem.nes";
        let mut deck = load(rom);
        deck.clock_seconds(4.0);
        assert_eq!(deck.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn exec_space_ppuio() {
        let rom = "test_roms/cpu/exec_space_ppuio.nes";
        let mut deck = load(rom);
        deck.clock_seconds(2.0);
        assert_eq!(deck.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn instr_timing() {
        let rom = "test_roms/cpu/instr_timing.nes";
        let mut deck = load(rom);
        deck.clock_seconds(23.0);
        assert_eq!(deck.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn apu_timing() {
        let rom = "test_roms/cpu/nestest.nes";
        let mut deck = load(rom);
        deck.cpu.bus.write(0x4017, 0x00);
        let mut irq_cycles = vec![];
        for _ in 0..=29840 {
            deck.clock();
            if deck.cpu.bus.apu.irq_pending {
                irq_cycles.push(deck.cpu.cycle_count);
                deck.cpu.bus.read(0x4015);
            }
        }
        assert_eq!(deck.cpu.cycle_count, 98172, "cpu cycle count should match");
        let frame_seq = deck.cpu.bus.apu.frame_sequencer();
        assert_eq!(
            frame_seq.divider.counter, 1626.5,
            "frame sequencer divider should match"
        );
        assert_eq!(
            frame_seq.sequencer.step, 2,
            "frame sequencer step should match"
        );
        assert_eq!(
            irq_cycles,
            vec![29831, 59662, 89491],
            "apu irq should occur on correct cycles"
        );
        assert!(!deck.cpu.bus.apu.irq_pending, "apu irq should be clear");
    }
}
