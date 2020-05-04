use tetanes::{
    apu::SAMPLE_RATE,
    bus::Bus,
    common::{Clocked, Powered},
    cpu::Cpu,
    mapper,
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
};
use wasm_bindgen::prelude::*;

mod utils;

#[wasm_bindgen]
pub struct Nes {
    paused: bool,
    cpu: Cpu,
}

#[wasm_bindgen]
impl Nes {
    pub fn new() -> Self {
        utils::set_panic_hook();
        utils::init_log();

        Self {
            paused: false,
            cpu: Cpu::init(Bus::new()),
        }
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn power_cycle(&mut self) {
        self.cpu.power_cycle();
    }

    pub fn frame(&self) -> *const u8 {
        self.cpu.bus.ppu.frame().as_ptr()
    }

    pub fn frame_len(&self) -> usize {
        self.cpu.bus.ppu.frame().len()
    }

    pub fn samples(&self) -> *const f32 {
        self.cpu.bus.apu.samples().as_ptr()
    }

    pub fn clear_samples(&mut self) {
        self.cpu.bus.apu.clear_samples();
    }

    pub fn samples_len(&self) -> usize {
        self.cpu.bus.apu.samples().len()
    }

    pub fn width(&self) -> u32 {
        RENDER_WIDTH
    }

    pub fn height(&self) -> u32 {
        RENDER_HEIGHT
    }

    pub fn sample_rate(&self) -> f32 {
        SAMPLE_RATE
    }

    pub fn clock_frame(&mut self) {
        if !self.paused {
            while !self.cpu.bus.ppu.frame_complete {
                let _ = self.cpu.clock();
            }
            self.cpu.bus.ppu.frame_complete = false;
        }
    }

    pub fn load_rom(&mut self, mut bytes: &[u8]) {
        let mapper = mapper::load_rom("file", &mut bytes).unwrap();
        self.cpu.bus.load_mapper(mapper);
    }

    pub fn handle_event(&mut self, key: &str, pressed: bool, repeat: bool) -> bool {
        if repeat {
            return false;
        }
        let mut gamepad = &mut self.cpu.bus.input.gamepad1;
        let mut matched = true;
        match key {
            "Escape" if pressed => self.toggle_pause(),
            "Enter" => gamepad.start = pressed,
            "Shift" => gamepad.select = pressed,
            "a" => gamepad.turbo_a = pressed,
            "s" => gamepad.turbo_b = pressed,
            "z" => gamepad.a = pressed,
            "x" => gamepad.b = pressed,
            "ArrowUp" => gamepad.up = pressed,
            "ArrowDown" => gamepad.down = pressed,
            "ArrowLeft" => gamepad.left = pressed,
            "ArrowRight" => gamepad.right = pressed,
            _ => matched = false,
        }
        matched
    }
}

impl Default for Nes {
    fn default() -> Self {
        Self::new()
    }
}
