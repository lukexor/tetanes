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
    bg_paused: bool,
    cpu: Cpu,
}

#[wasm_bindgen]
impl Nes {
    pub fn new() -> Self {
        utils::set_panic_hook();

        Self {
            paused: false,
            bg_paused: false,
            cpu: Cpu::init(Bus::new()),
        }
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
        while !self.cpu.bus.ppu.frame_complete {
            let _ = self.cpu.clock();
        }
        self.cpu.bus.ppu.frame_complete = false;
    }

    pub fn load_rom(&mut self, mut bytes: &[u8]) {
        let mapper = mapper::load_rom("file", &mut bytes).unwrap();
        self.cpu.bus.load_mapper(mapper);
    }

    pub fn cpu_info(&self) -> String {
        format!(
            "{:02X} A:{:02X} X:{:02X} Y:{:02X} P:{}, SP:{:02X} CYC:{}",
            self.cpu.pc,
            self.cpu.acc,
            self.cpu.x,
            self.cpu.y,
            self.cpu.status,
            self.cpu.sp,
            self.cpu.cycle_count,
        )
        .to_string()
    }

    pub fn start(&mut self, pressed: bool) {
        self.cpu.bus.input.gamepad1.start = pressed;
    }

    pub fn select(&mut self, pressed: bool) {
        self.cpu.bus.input.gamepad1.select = pressed;
    }

    pub fn a(&mut self, pressed: bool) {
        self.cpu.bus.input.gamepad1.a = pressed;
    }

    pub fn b(&mut self, pressed: bool) {
        self.cpu.bus.input.gamepad1.b = pressed;
    }

    pub fn up(&mut self, pressed: bool) {
        self.cpu.bus.input.gamepad1.up = pressed;
    }

    pub fn down(&mut self, pressed: bool) {
        self.cpu.bus.input.gamepad1.down = pressed;
    }

    pub fn left(&mut self, pressed: bool) {
        self.cpu.bus.input.gamepad1.left = pressed;
    }

    pub fn right(&mut self, pressed: bool) {
        self.cpu.bus.input.gamepad1.right = pressed;
    }
}
