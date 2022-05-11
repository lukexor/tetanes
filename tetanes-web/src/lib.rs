use tetanes::{
    audio::Audio,
    common::{Clocked, NesFormat, Powered},
    control_deck::ControlDeck,
    input::GamepadSlot,
    memory::RamState,
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
};
use wasm_bindgen::prelude::*;

mod utils;

const SAMPLE_RATE: f32 = 44_100.0;

#[wasm_bindgen]
pub struct Nes {
    paused: bool,
    control_deck: ControlDeck,
    audio: Audio,
}

#[wasm_bindgen]
impl Nes {
    pub fn init() {
        utils::set_panic_hook();
        utils::init_log();
    }

    pub fn new() -> Self {
        let control_deck = ControlDeck::new(NesFormat::default(), RamState::default());
        let sample_rate = control_deck.apu().sample_rate();
        Self {
            paused: true,
            control_deck,
            audio: Audio::new(sample_rate, SAMPLE_RATE),
        }
    }

    pub fn pause(&mut self, val: bool) {
        self.paused = val;
    }

    pub fn paused(&mut self) -> bool {
        self.paused
    }

    pub fn power_cycle(&mut self) {
        self.control_deck.power_cycle();
    }

    pub fn frame(&mut self) -> *const u8 {
        self.control_deck.frame_buffer().as_ptr()
    }

    pub fn frame_len(&mut self) -> usize {
        self.control_deck.frame_buffer().len()
    }

    pub fn samples(&mut self, sample_ratio: f32) -> *const f32 {
        let samples = self.control_deck.audio_samples();
        let output = self.audio.output(samples, sample_ratio);
        output.as_ptr()
    }

    pub fn clear_samples(&mut self) {
        self.control_deck.clear_audio_samples();
    }

    pub fn samples_len(&mut self) -> usize {
        self.audio.output_len()
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
            while !self.control_deck.frame_complete() {
                let _ = self.control_deck.clock();
            }
            self.control_deck.start_new_frame();
        }
    }

    pub fn load_rom(&mut self, mut bytes: &[u8]) {
        self.control_deck
            .load_rom("ROM", &mut bytes)
            .expect("valid rom");
        self.pause(false);
    }

    pub fn handle_event(&mut self, key: &str, pressed: bool, repeat: bool) -> bool {
        if repeat {
            return false;
        }
        let gamepad = &mut self.control_deck.gamepad_mut(GamepadSlot::One);
        let mut matched = true;
        match key {
            "Escape" if pressed => self.pause(!self.paused),
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
