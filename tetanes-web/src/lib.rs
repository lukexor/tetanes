use tetanes::{
    audio::{Audio, NesAudioCallback},
    common::{NesRegion, Powered},
    control_deck::ControlDeck,
    input::GamepadSlot,
    memory::RamState,
    ppu::{VideoFilter, RENDER_HEIGHT, RENDER_SIZE, RENDER_WIDTH},
};
use wasm_bindgen::prelude::*;

mod utils;

#[wasm_bindgen]
pub struct Nes {
    paused: bool,
    control_deck: ControlDeck,
    audio: Audio,
    buffer: Vec<f32>,
    callback: NesAudioCallback,
    sound: bool,
    dynamic_rate_control: bool,
    dynamic_rate_delta: f32,
}

#[wasm_bindgen]
impl Nes {
    pub fn init() {
        utils::set_panic_hook();
        utils::init_log();
    }

    pub fn new(output_sample_rate: f32, buffer_size: usize, max_delta: f32) -> Self {
        let mut control_deck = ControlDeck::new(NesRegion::Ntsc, RamState::default());
        control_deck.set_filter(VideoFilter::Pixellate);
        let input_sample_rate = control_deck.apu().sample_rate();
        let mut audio = Audio::new(input_sample_rate, output_sample_rate, 4096);
        let buffer = vec![0.0; buffer_size];
        let callback = audio.open_callback().expect("valid callback");
        Self {
            paused: true,
            control_deck,
            audio,
            buffer,
            callback,
            sound: true,
            dynamic_rate_control: true,
            dynamic_rate_delta: max_delta,
        }
    }

    pub fn pause(&mut self, val: bool) {
        self.paused = val;
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn set_sound(&mut self, enabled: bool) {
        self.sound = enabled;
    }

    pub fn power_cycle(&mut self) {
        self.control_deck.power_cycle();
    }

    pub fn dynamic_rate_control(&self) -> bool {
        self.dynamic_rate_control
    }

    pub fn set_dynamic_rate_control(&mut self, val: bool) {
        self.dynamic_rate_control = val;
    }

    pub fn dynamic_rate_delta(&self) -> f32 {
        self.dynamic_rate_delta
    }

    pub fn set_dynamic_rate_delta(&mut self, val: f32) {
        self.dynamic_rate_delta = val;
    }

    pub fn frame(&mut self) -> *const u8 {
        self.control_deck.frame_buffer().as_ptr()
    }

    pub fn frame_len(&self) -> usize {
        RENDER_SIZE as usize
    }

    pub fn samples(&mut self) -> *const f32 {
        self.callback.read(&mut self.buffer);
        self.buffer.as_ptr()
    }

    pub fn buffer_capacity(&self) -> usize {
        self.buffer.capacity()
    }

    pub fn width(&self) -> u32 {
        RENDER_WIDTH
    }

    pub fn height(&self) -> u32 {
        RENDER_HEIGHT
    }

    pub fn sample_rate(&self) -> f32 {
        self.audio.output_frequency()
    }

    pub fn clock_seconds(&mut self, seconds: f32) {
        self.control_deck
            .clock_seconds(seconds)
            .expect("valid clock");
        if self.sound {
            let samples = self.control_deck.audio_samples();
            self.audio
                .output(samples, self.dynamic_rate_control, self.dynamic_rate_delta);
        }
        self.control_deck.clear_audio_samples();
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
        Self::new(48_000.0, 4096, 0.005)
    }
}
