//! User Interface around the NES Console

use crate::{
    console::{Console, RENDER_HEIGHT, RENDER_WIDTH},
    input::{Input, InputRef},
    util, NesResult,
};
use pix_engine::{
    draw::Rect,
    pixel::{self, ColorType},
    sprite::Sprite,
    PixEngine, PixEngineResult, State, StateData,
};
use std::{cell::RefCell, collections::VecDeque, path::PathBuf, rc::Rc, time::Duration};

mod debug;
mod event;
mod menus;
mod settings;

pub use settings::UiSettings;

const APP_NAME: &str = "RustyNES";
const WINDOW_WIDTH: u32 = (RENDER_WIDTH as f32 * 8.0 / 7.0) as u32; // for 8:7 Aspect Ratio
const WINDOW_HEIGHT: u32 = RENDER_HEIGHT;
const REWIND_SIZE: u8 = 20;
const REWIND_TIMER: f64 = 5.0;

struct Message {
    timer: f64,
    timed: bool,
    text: String,
}

impl Message {
    pub fn new(text: &str) -> Self {
        Self {
            timer: 5.0,
            timed: true,
            text: text.to_string(),
        }
    }
    pub fn new_static(text: &str) -> Self {
        Self {
            timer: 0.0,
            timed: false,
            text: text.to_string(),
        }
    }
}

pub struct Ui {
    roms: Vec<PathBuf>,
    loaded_rom: PathBuf,
    paused: bool,
    turbo_clock: u8,
    input: InputRef,
    ctrl: bool,
    shift: bool,
    focused: bool,
    menu: bool,
    debug: bool,
    ppu_viewer: bool,
    nt_viewer: bool,
    ppu_viewer_window: Option<u32>,
    nt_viewer_window: Option<u32>,
    debug_sprite: Option<Sprite>,
    active_debug: bool,
    width: u32,
    height: u32,
    speed_counter: i32,
    rewind_timer: f64,
    rewind_slot: u8,
    rewind_save: u8,
    rewind_queue: VecDeque<u8>,
    console: Console,
    messages: Vec<Message>,
    settings: UiSettings,
}

impl Ui {
    pub fn new() -> Self {
        let settings = UiSettings::default();
        Self::with_settings(settings)
    }

    pub fn with_settings(settings: UiSettings) -> Self {
        let input = Rc::new(RefCell::new(Input::new()));
        let mut console = Console::init(input.clone(), settings.randomize_ram);
        console.debug(settings.debug);
        Self {
            roms: Vec::new(),
            loaded_rom: PathBuf::new(),
            paused: true,
            turbo_clock: 0,
            input,
            ctrl: false,
            shift: false,
            focused: true,
            menu: false,
            debug: settings.debug,
            ppu_viewer: false,
            nt_viewer: false,
            ppu_viewer_window: None,
            nt_viewer_window: None,
            debug_sprite: None,
            active_debug: false,
            width: settings.scale * WINDOW_WIDTH,
            height: settings.scale * WINDOW_HEIGHT,
            speed_counter: 0,
            rewind_timer: 3.0 * REWIND_TIMER,
            rewind_slot: 0,
            rewind_save: 0,
            rewind_queue: VecDeque::with_capacity(REWIND_SIZE as usize),
            console,
            messages: Vec::new(),
            settings,
        }
    }

    pub fn run(self) -> NesResult<()> {
        let width = self.width;
        let height = self.height;
        let vsync = self.settings.vsync;
        let mut engine = PixEngine::new(APP_NAME, self, width, height, vsync)?;
        engine.run()?;
        Ok(())
    }

    fn paused(&mut self, val: bool) {
        self.paused = val;
        // Disable PPU debug updating if we're not in active mode
        if !self.active_debug {
            self.console.debug(!val);
            self.console.cpu.mem.ppu.update_debug();
        }
        if self.paused {
            self.add_static_message("Paused");
        } else {
            self.remove_static_message("Paused");
        }
    }

    fn add_message(&mut self, text: &str) {
        self.messages.push(Message::new(text));
    }

    fn add_static_message(&mut self, text: &str) {
        self.messages.push(Message::new_static(text));
    }

    fn remove_static_message(&mut self, text: &str) {
        self.messages.retain(|msg| msg.text != text);
    }

    fn draw_messages(&mut self, elapsed: f64, data: &mut StateData) -> NesResult<()> {
        self.messages.retain(|msg| !msg.timed || msg.timer > 0.0);
        if !self.messages.is_empty() {
            let width = WINDOW_WIDTH * self.settings.scale - 20;
            let height = self.height;
            let message_box = Sprite::new(width, height);
            data.create_texture(
                1,
                "message",
                ColorType::Rgba,
                Rect::new(0, 0, width, height),
                Rect::new(10, 10, width, height),
            )?;
            data.set_draw_target(message_box);
            let mut y = self.height - 20 * data.get_font_scale();
            for msg in self.messages.iter_mut() {
                msg.timer -= elapsed;
                data.draw_string(2, y + 2, &msg.text, pixel::BLACK);
                data.draw_string(0, y, &msg.text, pixel::WHITE);
                y -= 10 * data.get_font_scale();
            }
            let target = data.take_draw_target().unwrap();
            let pixels = target.bytes();
            data.copy_texture(1, "message", &pixels)?;
        }
        Ok(())
    }
}

impl State for Ui {
    fn on_start(&mut self, data: &mut StateData) -> PixEngineResult<()> {
        if let Ok(mut roms) = util::find_roms(&self.settings.path) {
            self.roms.append(&mut roms);
        }
        if self.roms.len() == 1 {
            self.loaded_rom = self.roms[0].clone();
            self.console.load_rom(&self.loaded_rom)?;
            self.console.power_on()?;
            if self.settings.save_enabled {
                self.console.load_state(self.settings.save_slot)?;
            }
            let mut errors = Vec::new();
            for code in self.settings.genie_codes.iter() {
                if let Err(e) = self.console.add_genie_code(code) {
                    errors.push(e);
                }
            }
            for err in errors.iter() {
                self.add_message(&err.to_string());
            }
            self.paused = false;
            self.update_title(data);
        }

        data.create_texture(
            1,
            "nes",
            ColorType::Rgb,
            Rect::new(0, 8, RENDER_WIDTH, RENDER_HEIGHT - 8), // Trims overscan
            Rect::new(0, 0, self.width, self.height),
        )?;
        data.create_texture(
            1,
            "menu",
            ColorType::Rgba,
            Rect::new(0, 0, self.width, self.height),
            Rect::new(0, 0, self.width, self.height),
        )?;

        if self.debug {
            self.debug = false;
            self.toggle_debug(data)?;
        }
        if self.settings.fullscreen {
            data.fullscreen(true)?;
        }

        // Smooths out startup graphic glitches for some games
        if !self.paused {
            let startup_frames = 40;
            for _ in 0..startup_frames {
                self.console.clock_frame();
                if self.settings.sound_enabled {
                    let samples = self.console.audio_samples();
                    data.enqueue_audio(&samples);
                }
                self.console.clear_audio();
            }
        }
        Ok(())
    }

    fn on_update(&mut self, elapsed: Duration, data: &mut StateData) -> PixEngineResult<()> {
        let elapsed = elapsed.as_secs_f64();

        self.poll_events(data)?;
        self.update_title(data);

        // Save rewind snapshot
        self.rewind_timer -= elapsed;
        if self.rewind_timer <= 0.0 {
            self.rewind_save %= REWIND_SIZE;
            if self.rewind_save < 5 {
                self.rewind_save = 5;
            }
            self.rewind_timer = REWIND_TIMER;
            self.console.save_state(self.rewind_save)?;
            self.rewind_queue.push_back(self.rewind_save);
            self.rewind_save += 1;
            if self.rewind_queue.len() > REWIND_SIZE as usize {
                let _ = self.rewind_queue.pop_front();
            }
            self.rewind_slot = self.rewind_queue.len() as u8;
        }

        if !self.paused {
            // Frames that aren't multiples of the default render 1 more/less frames
            // every other frame
            let mut frames_to_run = 0;
            self.speed_counter += (100.0 * self.settings.speed) as i32;
            while self.speed_counter > 0 {
                self.speed_counter -= 100;
                frames_to_run += 1;
            }

            // Clock NES
            for _ in 0..frames_to_run as usize {
                self.console.clock_frame();
                self.turbo_clock = (1 + self.turbo_clock) % 6;
            }
        }

        // Update screen
        data.copy_texture(1, "nes", self.console.frame())?;
        if self.menu {
            self.draw_menu(data)?;
        }
        if self.debug {
            if self.active_debug || self.paused {
                self.draw_debug(data);
            }
            self.copy_debug(data)?;
        }
        if self.ppu_viewer {
            self.copy_ppu_viewer(data)?;
        }
        if self.nt_viewer {
            self.copy_nt_viewer(data)?;
        }
        self.draw_messages(elapsed, data)?;

        // Enqueue sound
        if self.settings.sound_enabled {
            let samples = self.console.audio_samples();
            data.enqueue_audio(&samples);
        }
        self.console.clear_audio();
        Ok(())
    }

    fn on_stop(&mut self, _data: &mut StateData) -> PixEngineResult<()> {
        self.console.power_off()?;
        Ok(())
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}
