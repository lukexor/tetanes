//! User Interface representing the the NES Control Deck

use crate::{
    apu::SAMPLE_RATE,
    bus::Bus,
    common::{Clocked, Powered},
    cpu::{Cpu, CPU_CLOCK_RATE},
    nes::{
        config::{DEFAULT_SPEED, MAX_SPEED, MIN_SPEED},
        debug::{DEBUG_WIDTH, INFO_HEIGHT, INFO_WIDTH},
        event::FrameEvent,
        menu::{Menu, MenuType, Message},
    },
    nes_err,
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    NesResult,
};
use include_dir::{include_dir, Dir};
use pix_engine::prelude::*;
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    path::PathBuf,
};

mod config;
mod debug;
mod event;
mod event_serialization;
mod menu;
mod state;

pub use config::NesConfig;

const APP_NAME: &str = "TetaNES";
// This includes static assets as a binary during installation
const _STATIC_DIR: Dir = include_dir!("./static");
const ICON_PATH: &str = "static/tetanes_icon.png";
const WINDOW_WIDTH: u32 = (RENDER_WIDTH as f32 * 8.0 / 7.0 + 0.5) as u32; // for 8:7 Aspect Ratio
const WINDOW_HEIGHT: u32 = RENDER_HEIGHT;
const REWIND_SLOT: u8 = 5;
const REWIND_SIZE: u8 = 5;
const REWIND_TIMER: f64 = 5.0;

pub struct Nes {
    roms: Vec<PathBuf>,
    loaded_rom: PathBuf,
    paused: bool,
    background_pause: bool,
    running_time: f64,
    turbo: bool,
    turbo_clock: u8,
    cpu: Cpu,
    cycles_remaining: f32,
    zapper_decay: u32,
    focused_window: Option<WindowId>,
    menus: [Menu; 4],
    held_keys: HashMap<u8, bool>,
    cpu_break: bool,
    break_instr: Option<u16>,
    should_close: bool,
    nes_window: WindowId,
    ppu_viewer_window: Option<WindowId>,
    nt_viewer_window: Option<WindowId>,
    ppu_viewer: bool,
    nt_viewer: bool,
    nt_scanline: u32,
    pat_scanline: u32,
    screen: usize,
    debug_image: Image,
    ppu_info_image: Image,
    nt_info_image: Image,
    active_debug: bool,
    width: u32,
    height: u32,
    speed_counter: i32,
    rewind_timer: f64,
    rewind_queue: VecDeque<u8>,
    recording: bool,
    playback: bool,
    frame: usize,
    replay_buffer: Vec<FrameEvent>,
    messages: Vec<Message>,
    config: NesConfig,
}

impl Nes {
    // Create a new NES emulation with default config settings
    pub fn new() -> Self {
        let config = NesConfig::default();
        Self::with_config(config).unwrap()
    }

    /// Create a new NES emulation with passed in config settings
    pub fn with_config(config: NesConfig) -> NesResult<Self> {
        let scale = config.scale;
        let width = scale * WINDOW_WIDTH;
        let height = scale * WINDOW_HEIGHT;
        let cpu = Cpu::init(Bus::new());
        let mut nes = Self {
            roms: Vec::new(),
            loaded_rom: PathBuf::new(),
            paused: true,
            background_pause: false,
            running_time: 0.0,
            turbo: false,
            turbo_clock: 0,
            cpu,
            cycles_remaining: 0.0,
            zapper_decay: 0,
            focused_window: None,
            menus: [
                Menu::new(MenuType::Config, width, height),
                Menu::new(MenuType::Help, width, height),
                Menu::new(MenuType::Keybind, width, height),
                Menu::new(MenuType::OpenRom, width, height),
            ],
            held_keys: HashMap::new(),
            cpu_break: false,
            break_instr: None,
            should_close: false,
            nes_window: WindowId::default(),
            ppu_viewer_window: None,
            nt_viewer_window: None,
            ppu_viewer: false,
            nt_viewer: false,
            nt_scanline: 0,
            pat_scanline: 0,
            screen: 0,
            debug_image: Image::rgb(DEBUG_WIDTH, height),
            ppu_info_image: Image::rgb(INFO_WIDTH, INFO_HEIGHT),
            nt_info_image: Image::rgb(INFO_WIDTH, INFO_HEIGHT),
            active_debug: false,
            width,
            height,
            speed_counter: 0,
            rewind_timer: REWIND_TIMER,
            rewind_queue: VecDeque::with_capacity(REWIND_SIZE as usize),
            recording: config.record,
            playback: false,
            frame: 0,
            replay_buffer: Vec::new(),
            messages: Vec::new(),
            config,
        };
        if nes.config.replay.is_some() {
            nes.playback = true;
            nes.replay_buffer = nes.load_replay()?;
        }
        Ok(nes)
    }

    /// Begins emulation by starting the game engine loop
    pub fn run(&mut self) -> NesResult<()> {
        let width = self.width;
        let height = self.height;

        // Extract title from filename
        let mut path = self.config.path.to_owned();
        path.set_extension("");
        let filename = path.file_name().and_then(|f| f.to_str());
        let title = if let Some(filename) = filename {
            format!("{} - {}", APP_NAME, filename)
        } else {
            APP_NAME.to_owned()
        };

        let mut engine = PixEngine::create(width, height);
        engine.with_title(title);
        engine.with_frame_rate();
        engine.audio_sample_rate(SAMPLE_RATE.round() as i32);
        engine.icon(ICON_PATH);
        engine.resizable();
        if self.config.vsync {
            engine.vsync_enabled();
        }
        engine.build()?.run(self)?;
        Ok(())
    }

    /// Steps the console the number of instructions required to generate an entire frame
    pub fn clock_frame(&mut self) {
        while !self.cpu_break && !self.cpu.bus.ppu.frame_complete {
            let _ = self.clock();
        }
        self.cpu_break = false;
        self.cpu.bus.ppu.frame_complete = false;
        self.turbo_clock = (self.turbo_clock + 1) % 6;
        self.frame += 1;
    }

    /// Steps the console the number of seconds
    pub fn clock_seconds(&mut self, seconds: f32) {
        self.cycles_remaining += CPU_CLOCK_RATE * seconds;
        while !self.cpu_break && self.cycles_remaining > 0.0 {
            self.cycles_remaining -= self.clock() as f32;
        }
        if self.cpu_break {
            self.cycles_remaining = 0.0;
        }
        self.cpu_break = false;
    }

    /// Finds roms in the current path. If there is only one, it is started
    fn find_or_load_roms(&mut self, s: &mut PixState) -> NesResult<()> {
        match self.find_roms() {
            Ok(mut roms) => self.roms.append(&mut roms),
            Err(e) => nes_err!("{}", e)?,
        }
        if self.roms.len() == 1 {
            self.load_rom(0)?;
            self.power_on();

            if self.config.clear_save {
                if let Ok(save_path) = state::save_path(&self.loaded_rom, self.config.save_slot) {
                    if save_path.exists() {
                        let _ = std::fs::remove_file(&save_path);
                        self.add_message(&format!("Cleared Save Slot {}", self.config.save_slot));
                    }
                }
            } else {
                let rewind = false;
                self.load_state(self.config.save_slot, rewind);
            }

            let codes = self.config.genie_codes.to_vec();
            for code in codes {
                if let Err(e) = self.cpu.bus.add_genie_code(&code) {
                    self.add_message(&e.to_string());
                }
            }
            self.update_title(s)?;
        }
        Ok(())
    }

    /// Sets up the emulation based on startup configuration settings
    fn config_setup(&mut self, s: &mut PixState) -> NesResult<()> {
        if self.config.debug {
            self.config.debug = !self.config.debug;
            self.toggle_debug(s)?;
        }
        if self.config.speed < MIN_SPEED {
            self.config.speed = MIN_SPEED;
        } else if self.config.speed > MAX_SPEED {
            self.config.speed = MAX_SPEED;
        } else {
            // Round to two decimal places
            self.config.speed = (self.config.speed * 100.0).round() / 100.0;
        }
        self.cpu.bus.apu.set_speed(self.config.speed);
        if self.config.fullscreen {
            s.fullscreen(true);
        }
        Ok(())
    }

    /// Runs the emulation a certain amount if not paused based on settings
    fn run_emulation(&mut self, elapsed: f64) {
        if !self.paused {
            self.running_time += elapsed;
            // Frames that aren't multiples of the default render 1 more/less frames
            // every other frame
            let mut frames_to_run = 0;
            self.speed_counter += (100.0 * self.config.speed) as i32;
            while self.speed_counter > 0 {
                self.speed_counter -= 100;
                frames_to_run += 1;
            }
            // Clock NES
            for _ in 0..frames_to_run as usize {
                self.clock_frame();
            }
        }
    }

    /// Update rendering textures with emulation state
    fn update_textures(&mut self, s: &mut PixState) -> PixResult<()> {
        // Update main screen
        s.update_texture(
            self.screen,
            Some(rect!(0, 0, RENDER_WIDTH, RENDER_HEIGHT)),
            &self.cpu.bus.ppu.frame(),
            3 * RENDER_WIDTH as usize,
        )?;
        s.draw_texture(
            self.screen,
            Some(rect!(0, 8, RENDER_WIDTH, RENDER_HEIGHT - 8)),
            Some(rect!(0, 0, self.width, self.height)),
        )?;
        // s.image_resized(0, 0, s.width(), s.height(), &self.screen)?;

        // Draw any open menus
        for menu in self.menus.iter_mut() {
            menu.draw(s)?;
        }
        self.draw_messages(s)?;
        if self.config.debug {
            // Draw updated debug info if active_debug is set, or if the game
            // gets paused
            if self.active_debug || self.paused {
                self.draw_debug(s)?;
            }
            self.copy_debug(s)?;
        }
        if self.ppu_viewer {
            self.copy_ppu_viewer(s)?;
        }
        if self.nt_viewer {
            self.copy_nt_viewer(s)?;
        }
        Ok(())
    }
}

impl AppState for Nes {
    fn on_start(&mut self, s: &mut PixState) -> PixResult<()> {
        self.nes_window = s.window_id();
        self.focused_window = Some(self.nes_window);
        self.create_textures(s)?;
        self.find_or_load_roms(s)?;
        self.config_setup(s)?;
        Ok(())
    }

    fn on_update(&mut self, s: &mut PixState) -> PixResult<()> {
        self.clock_turbo();
        if self.should_close {
            return Ok(());
        }
        self.update_title(s)?;
        self.check_window_focus();
        self.save_rewind(s.delta_time());
        self.run_emulation(s.delta_time());
        self.update_textures(s)?;
        // Enqueue sound
        if self.config.sound_enabled {
            let samples = self.cpu.bus.apu.samples();
            s.enqueue_audio(&samples);
        }
        self.cpu.bus.apu.clear_samples();
        Ok(())
    }

    fn on_stop(&mut self, _s: &mut PixState) -> PixResult<()> {
        self.power_off();
        Ok(())
    }

    fn on_key_pressed(&mut self, s: &mut PixState, key: Key, repeat: bool) -> PixResult<()> {
        // if self.recording {
        //     self.replay_buffer
        //         .push(FrameEvent::new(self.frame, events.clone()));
        // }
        // Only process event if we're focused
        if !self.playback && self.focused_window.is_none() {
            return Ok(());
        }
        if repeat {
            self.handle_keyrepeat(key);
        } else {
            self.handle_keydown(s, key)?;
        }
        Ok(())
    }

    fn on_key_released(&mut self, _s: &mut PixState, key: Key, _repeat: bool) -> PixResult<()> {
        self.held_keys.insert(key as u8, false);
        match key {
            Key::Space => {
                self.config.speed = DEFAULT_SPEED;
                self.cpu.bus.apu.set_speed(self.config.speed);
            }
            _ => self.handle_input_event(key, false),
        }
        Ok(())
    }

    // fn on_controller_down() {}
    // fn on_controller_release() {}
    // fn on_controller_axis_motion() {}
}

impl fmt::Debug for Nes {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Nes {{\n  cpu: {:?}\n}} ", self.cpu)
    }
}

impl Default for Nes {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemRead;
    use std::path::PathBuf;

    fn load(file: &str) -> Nes {
        let mut nes = Nes::new();
        nes.roms.push(PathBuf::from(file));
        nes.load_rom(0).unwrap();
        nes.power_on();
        nes
    }

    #[test]
    #[cfg(feature = "no-randomize-ram")]
    fn nestest() {
        let rom = "tests/cpu/nestest.nes";
        let mut nes = load(&rom);
        nes.cpu.pc = 0xC000; // Start automated tests
        let _ = nes.clock_seconds(1.0);
        assert_eq!(nes.cpu.peek(0x0000), 0x00, "{}", rom);
    }

    #[test]
    fn dummy_writes_oam() {
        let rom = "tests/cpu/dummy_writes_oam.nes";
        let mut nes = load(&rom);
        let _ = nes.clock_seconds(6.0);
        assert_eq!(nes.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn dummy_writes_ppumem() {
        let rom = "tests/cpu/dummy_writes_ppumem.nes";
        let mut nes = load(&rom);
        let _ = nes.clock_seconds(4.0);
        assert_eq!(nes.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn exec_space_ppuio() {
        let rom = "tests/cpu/exec_space_ppuio.nes";
        let mut nes = load(&rom);
        let _ = nes.clock_seconds(2.0);
        assert_eq!(nes.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    #[cfg(feature = "no-randomize-ram")]
    fn instr_timing() {
        let rom = "tests/cpu/instr_timing.nes";
        let mut nes = load(&rom);
        let _ = nes.clock_seconds(23.0);
        assert_eq!(nes.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn apu_timing() {
        // TODO assert outputs
        let rom = "tests/cpu/nestest.nes";
        let mut nes = load(&rom);
        for _ in 0..=29840 {
            let apu = &nes.cpu.bus.apu;
            println!(
                "{}: counter: {}, step: {}, irq: {}",
                nes.cpu.cycle_count,
                apu.frame_sequencer.divider.counter,
                apu.frame_sequencer.sequencer.step,
                apu.irq_pending
            );
            nes.clock();
        }
    }
}
