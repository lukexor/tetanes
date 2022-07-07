use tetanes::{
    audio::{AudioMixer, NesAudioCallback},
    control_deck::ControlDeck,
    input::GamepadSlot,
    memory::RamState,
    ppu::{VideoFilter, RENDER_HEIGHT, RENDER_WIDTH},
};
use wasm_bindgen::prelude::*;

mod utils;

#[wasm_bindgen]
pub struct Nes {
    paused: bool,
    control_deck: ControlDeck,
    audio: AudioMixer,
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

    pub fn new(output_sample_rate: f32, max_delta: f32) -> Self {
        let mut control_deck = ControlDeck::new(RamState::default());
        control_deck.set_filter(VideoFilter::Pixellate);
        let input_sample_rate = control_deck.sample_rate();
        let mut audio = AudioMixer::new(input_sample_rate, output_sample_rate, 4096);
        let callback = audio.open_callback().expect("valid callback");
        Self {
            paused: true,
            control_deck,
            audio,
            callback,
            sound: true,
            dynamic_rate_control: true,
            dynamic_rate_delta: max_delta,
        }
    }

    pub fn pause(&mut self, val: bool) {
        self.paused = val;
    }

    pub fn set_sound(&mut self, enabled: bool) {
        self.sound = enabled;
    }

    pub fn copy_frame(&mut self, frame: &mut [u8]) {
        frame.copy_from_slice(self.control_deck.frame_buffer());
    }

    pub fn audio_callback(&mut self, out: &mut [f32]) {
        self.callback.read(out);
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

    pub fn clock_frame(&mut self) {
        self.control_deck.clock_frame().expect("valid clock");
        if self.sound {
            let samples = self.control_deck.audio_samples();
            self.audio
                .consume(samples, self.dynamic_rate_control, self.dynamic_rate_delta);
        }
        self.control_deck.clear_audio_samples();
    }

    pub fn load_rom(&mut self, mut bytes: &[u8]) {
        self.control_deck
            .load_rom("ROM", &mut bytes)
            .expect("valid rom");
        self.callback.clear();
    }

    pub fn handle_event(&mut self, key: &str, pressed: bool, repeat: bool) -> bool {
        if repeat {
            return false;
        }
        let gamepad = &mut self.control_deck.gamepad_mut(GamepadSlot::One);
        let mut matched = true;
        match key {
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
        Self::new(44_100.0, 0.005)
    }
}
