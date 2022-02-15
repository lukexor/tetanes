use crate::{
    apu::AudioChannel,
    bus::Bus,
    common::{Addr, Clocked, Powered},
    cpu::{instr::Instr, Cpu, CPU_CLOCK_RATE},
    input::{Gamepad, GamepadSlot},
    mapper,
    memory::MemAccess,
    ppu::VideoFormat,
    NesResult,
};
use std::io::Read;

/// Represents an NES Control Deck
#[derive(Debug, Clone)]
#[must_use]
pub struct ControlDeck {
    running: bool,
    consistent_ram: bool,
    loaded_rom: Option<String>,
    turbo_clock: usize,
    cycles_remaining: f32,
    cpu: Cpu,
}

impl ControlDeck {
    /// Creates a new `ControlDeck` instance.
    pub fn new(consistent_ram: bool) -> Self {
        let cpu = Cpu::init(Bus::new(consistent_ram));
        Self {
            running: false,
            consistent_ram,
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
        let mapper = mapper::load_rom(name, rom, self.consistent_ram)?;
        self.cpu.bus.load_mapper(mapper);
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
        let mut total_ticks = 0;
        while self.cycles_remaining > 0.0 {
            let ticks = self.cpu.clock();
            total_ticks += ticks;
            self.cycles_remaining -= ticks as f32;
        }
        total_ticks
    }

    /// Steps the control deck a single clock cycle.
    pub fn clock_cpu(&mut self) -> usize {
        self.cpu.clock()
    }

    /// Steps the control deck a single scanline.
    pub fn clock_scanline(&mut self) -> usize {
        let current_scanline = self.cpu.bus.ppu.scanline;
        let mut total_ticks = 0;
        while self.cpu.bus.ppu.scanline == current_scanline {
            total_ticks += self.clock_cpu();
        }
        total_ticks
    }

    /// Returns whether the CPU is corrupted or not.
    pub fn cpu_corrupted(&self) -> bool {
        self.cpu.corrupted
    }

    /// Returns the current CPU program counter.
    pub fn pc(&self) -> Addr {
        self.cpu.pc
    }

    /// Returns the next CPU instruction to be executed.
    pub fn next_instr(&self) -> Instr {
        self.cpu.next_instr()
    }

    /// Returns the next address on the bus to be either read or written to along with the current
    /// value at the target address.
    pub fn next_addr(&self, access: MemAccess) -> (Option<Addr>, Option<u16>) {
        self.cpu.next_addr(access)
    }

    /// Disassemble an address range of CPU instructions.
    pub fn disasm(&self, start: Addr, end: Addr) -> Vec<String> {
        let mut disassembly = Vec::with_capacity(256);
        let mut addr = start;
        while addr <= end {
            disassembly.push(self.cpu.disassemble(&mut addr));
        }
        disassembly
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
        let consistent_ram = true;
        Self::new(consistent_ram)
    }
}

impl Clocked for ControlDeck {
    /// Steps the control deck an entire frame
    fn clock(&mut self) -> usize {
        self.clock_turbo();
        while !self.frame_complete() {
            self.cpu.clock();
        }
        self.start_new_frame();
        1
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
    use crate::memory::MemRead;
    use std::{fs::File, io::BufReader, path::PathBuf};

    fn load(file: &str) -> ControlDeck {
        let consistent_ram = true;
        let mut deck = ControlDeck::new(consistent_ram);
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
        for _ in 0..=29840 {
            let apu = &deck.cpu.bus.apu;
            println!(
                "{}: counter: {}, step: {}, irq: {}",
                deck.cpu.cycle_count,
                apu.frame_sequencer().divider.counter,
                apu.frame_sequencer().sequencer.step,
                apu.irq_pending
            );
            deck.cpu.clock();
        }
    }
}
