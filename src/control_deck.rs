use crate::{
    bus::Bus,
    common::{Clocked, Powered},
    cpu::{Cpu, CPU_CLOCK_RATE},
    input::Gamepad,
    mapper, NesResult,
};
use std::io::Read;

/// Represents an NES Control Deck
#[derive(Debug, Clone)]
pub struct ControlDeck {
    loaded_rom: Option<String>,
    cpu: Cpu,
    running: bool,
    cycles_remaining: f32,
    turbo_clock: usize,
}

impl ControlDeck {
    /// Creates a new ControlDeck instance.
    pub fn new() -> Self {
        let cpu = Cpu::init(Bus::new());
        Self {
            loaded_rom: None,
            cpu,
            running: false,
            cycles_remaining: 0.0,
            turbo_clock: 0,
        }
    }

    /// Loads a ROM cartridge into memory
    pub fn load_rom<F: Read>(&mut self, name: &str, rom: &mut F) -> NesResult<()> {
        self.loaded_rom = Some(name.to_owned());
        let mapper = mapper::load_rom(name, rom)?;
        self.cpu.bus.load_mapper(mapper);
        Ok(())
    }

    /// Get a frame worth of pixels
    pub fn get_frame(&self) -> &[u8] {
        self.cpu.bus.ppu.frame()
    }

    /// Get audio samples.
    pub fn get_audio_samples(&self) -> &[f32] {
        self.cpu.bus.apu.samples()
    }

    /// Clear audio samples.
    pub fn clear_audio_samples(&mut self) {
        self.cpu.bus.apu.clear_samples();
    }

    /// Steps the control deck the number of seconds
    pub fn clock_seconds(&mut self, seconds: f32) {
        self.cycles_remaining += CPU_CLOCK_RATE * seconds;
        while self.cycles_remaining > 0.0 {
            self.cycles_remaining -= self.cpu.clock() as f32;
        }
    }

    /// Returns a mutable reference to gamepad1.
    pub fn get_gamepad1_mut(&mut self) -> &mut Gamepad {
        &mut self.cpu.bus.input.gamepad1
    }

    /// Returns a mutable reference to gamepad2.
    pub fn get_gamepad2_mut(&mut self) -> &mut Gamepad {
        &mut self.cpu.bus.input.gamepad2
    }

    fn clock_turbo(&mut self) {
        self.turbo_clock += 1;
        if self.turbo_clock > 3 {
            self.turbo_clock = 0;
        }
        let turbo = self.turbo_clock == 0;
        let mut input = &mut self.cpu.bus.input;
        if input.gamepad1.turbo_a {
            input.gamepad1.a = turbo;
        }
        if input.gamepad1.turbo_b {
            input.gamepad1.b = turbo;
        }
        if input.gamepad2.turbo_a {
            input.gamepad2.a = turbo;
        }
        if input.gamepad2.turbo_b {
            input.gamepad2.b = turbo;
        }
    }
}

impl Default for ControlDeck {
    fn default() -> Self {
        Self::new()
    }
}

impl Clocked for ControlDeck {
    /// Steps the control deck an entire frame
    fn clock(&mut self) -> usize {
        self.clock_turbo();
        while !self.cpu.bus.ppu.frame_complete {
            self.cpu.clock();
        }
        self.cpu.bus.ppu.frame_complete = false;
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
        let mut deck = ControlDeck::new();
        let rom = File::open(PathBuf::from(file)).unwrap();
        let mut rom = BufReader::new(rom);
        deck.load_rom(file, &mut rom).unwrap();
        deck.power_on();
        deck
    }

    #[test]
    #[cfg(feature = "no-randomize-ram")]
    fn nestest() {
        let rom = "tests/cpu/nestest.nes";
        let mut deck = load(&rom);
        deck.cpu.pc = 0xC000; // Start automated tests
        deck.clock_seconds(1.0);
        assert_eq!(deck.cpu.peek(0x0000), 0x00, "{}", rom);
    }

    #[test]
    fn dummy_writes_oam() {
        let rom = "tests/cpu/dummy_writes_oam.nes";
        let mut deck = load(&rom);
        deck.clock_seconds(6.0);
        assert_eq!(deck.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn dummy_writes_ppumem() {
        let rom = "tests/cpu/dummy_writes_ppumem.nes";
        let mut deck = load(&rom);
        deck.clock_seconds(4.0);
        assert_eq!(deck.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn exec_space_ppuio() {
        let rom = "tests/cpu/exec_space_ppuio.nes";
        let mut deck = load(&rom);
        deck.clock_seconds(2.0);
        assert_eq!(deck.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    #[cfg(feature = "no-randomize-ram")]
    fn instr_timing() {
        let rom = "tests/cpu/instr_timing.nes";
        let mut deck = load(&rom);
        deck.clock_seconds(23.0);
        assert_eq!(deck.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn apu_timing() {
        // TODO assert outputs
        let rom = "tests/cpu/nestest.nes";
        let mut deck = load(&rom);
        for _ in 0..=29840 {
            let apu = &deck.cpu.bus.apu;
            println!(
                "{}: counter: {}, step: {}, irq: {}",
                deck.cpu.cycle_count,
                apu.frame_sequencer.divider.counter,
                apu.frame_sequencer.sequencer.step,
                apu.irq_pending
            );
            deck.cpu.clock();
        }
    }
}
