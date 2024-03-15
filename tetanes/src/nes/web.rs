use crate::{
    audio::{filter::Filter, Audio, Mixer},
    control_deck::ControlDeck,
    input::{JoypadBtnState, Player},
    logging,
    nes::config::{Config, SampleRate},
    ppu::Ppu,
};
use std::iter;
use wasm_bindgen::prelude::*;
use web_time::Duration;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn debug(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    fn warn(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    fn error(s: &str);
}

#[macro_use]
pub mod log {
    #[macro_export]
    macro_rules! debug {
        ($($t:tt)*) => {
            unsafe { debug(&format_args!($($t)*).to_string()) }
        }
    }
    #[macro_export]
    macro_rules! warn {
        ($($t:tt)*) => {
            unsafe { warn(&format_args!($($t)*).to_string()) }
        }
    }
    #[macro_export]
    macro_rules! error {
        ($($t:tt)*) => (unsafe { error(&format_args!($($t)*).to_string()) })
    }
}

#[derive(Debug)]
#[must_use]
pub struct Stats {
    clock_times: Vec<web_time::Duration>,
    queued_audio: Vec<web_time::Duration>,
    last_printed: web_time::Instant,
}

impl Stats {
    fn new() -> Self {
        Self {
            clock_times: Vec::with_capacity(1024),
            queued_audio: Vec::with_capacity(1024),
            last_printed: web_time::Instant::now(),
        }
    }
}

#[derive(Debug)]
#[wasm_bindgen]
pub struct Nes {
    paused: bool,
    config: Config,
    control_deck: ControlDeck,
    mixer: Audio,
    sample_avg: f32,
    sample_count: f32,
    decim_fraction: f32,
    filters: [Filter; 3],
    processed_samples: Vec<f32>,
    stats: Stats,
}

impl Nes {
    pub fn queued_time(&self) -> Duration {
        let queued_time = self.processed_samples.len() as f32 / 44100.0;
        Duration::from_secs_f32(queued_time)
    }
}

#[wasm_bindgen]
impl Nes {
    pub fn new() -> Self {
        let _guard = logging::init();
        let mut config = Config::default();
        let control_deck = ControlDeck::new();
        let sample_rate = config.audio_sample_rate;
        config.audio_latency = Duration::from_millis(60);
        let mixer = Audio::new(
            control_deck.clock_rate() * f32::from(config.frame_speed),
            sample_rate,
            config.audio_latency,
            config.audio_buffer_size,
        );
        let sample_latency = (f32::from(sample_rate) * config.audio_latency.as_secs_f32()) as usize;
        Self {
            paused: true,
            config,
            control_deck,
            mixer,
            sample_avg: 0.0,
            sample_count: 0.0,
            decim_fraction: 0.0,
            filters: [
                Filter::high_pass(sample_rate, 90.0, 1500.0),
                Filter::high_pass(sample_rate, 440.0, 1500.0),
                Filter::low_pass(sample_rate, 11_000.0, 1500.0),
            ],
            processed_samples: Vec::with_capacity(
                sample_latency.max((f32::from(SampleRate::MAX) / 60.0 * 2.0 * 2.0) as usize),
            ),
            stats: Stats::new(),
        }
    }

    pub fn pause(&mut self, val: bool) {
        self.paused = val;
        if let Err(err) = self.mixer.pause(self.paused) {
            error!("pause audio error: {err}");
        }
    }

    pub fn frame(&mut self) -> *const u8 {
        self.control_deck.frame_buffer().as_ptr()
    }

    pub fn width(&self) -> u32 {
        Ppu::WIDTH
    }

    pub fn height(&self) -> u32 {
        Ppu::HEIGHT
    }

    pub fn enable_audio(&mut self, enabled: bool) {
        if let Err(err) = self.mixer.pause(!enabled) {
            error!("enable audio error: {err}");
        }
    }

    pub fn clock_frame(&mut self) {
        if self.paused {
            return;
        }

        while self.queued_time() <= self.config.audio_latency {
            self.stats.queued_audio.push(self.queued_time());
            // debug!(
            //     "queued_audio_time: {:.4}s",
            //     self.queued_time().as_secs_f32()
            // );

            let start = web_time::Instant::now();
            match self.control_deck.clock_frame() {
                Ok(_) => {
                    // if let Err(err) = self.mixer.process(self.control_deck.audio_samples()) {
                    //     error!("process error: {err}");
                    // }
                    Mixer::downsample(
                        self.control_deck.audio_samples(),
                        self.mixer.channels(),
                        self.mixer.resample_ratio(),
                        &mut self.processed_samples,
                        &mut self.filters,
                        &mut self.sample_avg,
                        &mut self.sample_count,
                        &mut self.decim_fraction,
                    );
                    self.control_deck.clear_audio_samples();
                }
                Err(err) => {
                    error!("clock error: {err}");
                    self.pause(true);
                }
            }
            self.stats.clock_times.push(start.elapsed());
            // debug!("clock time: {:.4}s", start.elapsed().as_secs_f32());
            // debug!("queued_time: {:.4}s", self.queued_time().as_secs_f32());
        }
        if self.stats.last_printed.elapsed() >= Duration::from_secs(1) {
            let clock_times = &self.stats.clock_times;
            let queued_audio = &self.stats.queued_audio;
            let clock_len = clock_times.len();
            let clock_avg = clock_times.iter().sum::<Duration>() / clock_times.len() as u32;
            let audio_avg = queued_audio.iter().sum::<Duration>() / queued_audio.len() as u32;
            debug!(
                "times: {clock_len}, clock_avg: {:.4}s, audio_avg: {:.4}s",
                clock_avg.as_secs_f32(),
                audio_avg.as_secs_f32()
            );
            self.stats.clock_times.clear();
            self.stats.queued_audio.clear();
            self.stats.last_printed = web_time::Instant::now();
        }
    }

    pub fn audio_callback(&mut self, out: &mut [f32]) {
        let num_samples = out.len();
        let len = num_samples.min(self.processed_samples.len());
        if len < num_samples {
            warn!("audio underflow: {} < {}", len, num_samples);
        }
        for (out, sample) in out
            .iter_mut()
            .zip(self.processed_samples.drain(..len).chain(iter::repeat(0.0)))
        {
            *out = sample;
        }
    }

    pub fn load_rom(&mut self, mut bytes: &[u8]) {
        self.control_deck
            .load_rom("ROM", &mut bytes, Some(&self.config.deck))
            .expect("valid rom");
        self.paused = false;
        if let Err(err) = self.mixer.start() {
            error!("load rom error: {err}");
        }
    }

    pub fn handle_event(&mut self, key: &str, pressed: bool, repeat: bool) -> bool {
        if repeat {
            return false;
        }
        let joypad = &mut self.control_deck.joypad_mut(Player::One);
        let mut matched = true;
        match key {
            "Enter" => joypad.set_button(JoypadBtnState::START, pressed),
            "Shift" => joypad.set_button(JoypadBtnState::SELECT, pressed),
            "a" => joypad.set_button(JoypadBtnState::TURBO_A, pressed),
            "s" => joypad.set_button(JoypadBtnState::TURBO_B, pressed),
            "z" => joypad.set_button(JoypadBtnState::A, pressed),
            "x" => joypad.set_button(JoypadBtnState::B, pressed),
            "ArrowUp" => joypad.set_button(JoypadBtnState::UP, pressed),
            "ArrowDown" => joypad.set_button(JoypadBtnState::DOWN, pressed),
            "ArrowLeft" => joypad.set_button(JoypadBtnState::LEFT, pressed),
            "ArrowRight" => joypad.set_button(JoypadBtnState::RIGHT, pressed),
            _ => matched = false,
        }
        matched
    }
}

#[wasm_bindgen]
pub fn wasm_memory() -> JsValue {
    wasm_bindgen::memory()
}

impl Default for Nes {
    fn default() -> Self {
        Self::new()
    }
}
