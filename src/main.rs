use pix_engine::{
    draw::Rect,
    event::{Key, PixEvent},
    pixel::{ColorType, Sprite},
    PixEngine, PixEngineResult, *,
};
use rustynes::{
    console::{cpu::StatusRegs, Console, RENDER_HEIGHT, RENDER_WIDTH},
    input::{Input, InputRef},
    util,
};
use std::{cell::RefCell, env, path::PathBuf, rc::Rc, time::Duration};

const DEFAULT_TITLE: &str = "RustyNES";
const WINDOW_WIDTH: u32 = (RENDER_WIDTH as f32 * 8.0 / 7.0) as u32; // for 8:7 Aspect Ratio
const WINDOW_HEIGHT: u32 = RENDER_HEIGHT;
const DEFAULT_SPEED: f64 = 1.0; // 100% - 60 Hz
const MIN_SPEED: f64 = 0.25; // 25% - 240 Hz
const MAX_SPEED: f64 = 2.0; // 200% - 30 Hz

pub fn main() {
    let ui = Ui::new();
    let width = ui.width;
    let height = ui.height;
    let mut engine = PixEngine::new(DEFAULT_TITLE, ui, width, height);
    engine.vsync(true);
    engine.run().unwrap_or_else(|err| {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    });
}

struct UiSettings {
    save_slot: u8,
    save_enabled: bool,
    sound_enabled: bool,
    concurrent_dpad: bool,
    randomize_ram: bool,
    fullscreen: bool,
    vsync: bool,
    speed: f64,
}

impl UiSettings {
    fn new() -> Self {
        Self {
            save_slot: 1,
            save_enabled: true,
            sound_enabled: true,
            concurrent_dpad: false,
            randomize_ram: false,
            fullscreen: false,
            vsync: false,
            speed: 1.0,
        }
    }
}

struct Ui {
    current_dir: PathBuf,
    roms: Vec<PathBuf>,
    loaded_rom: PathBuf,
    running: bool,
    debug: bool,
    ppu_debug: bool,
    turbo_clock: u8,
    input: InputRef,
    ctrl: bool,
    shift: bool,
    width: u32,
    height: u32,
    console: Console,
    settings: UiSettings,
}

impl Ui {
    fn new() -> Self {
        let settings = UiSettings::default();
        let input = Rc::new(RefCell::new(Input::new()));
        let console = Console::init(input.clone(), settings.randomize_ram);
        Self {
            current_dir: env::current_dir().unwrap_or_default(),
            roms: Vec::new(),
            loaded_rom: PathBuf::new(),
            running: false,
            debug: false,
            ppu_debug: false,
            turbo_clock: 0,
            input,
            ctrl: false,
            shift: false,
            width: 3 * WINDOW_WIDTH,
            height: 3 * WINDOW_HEIGHT,
            console,
            settings,
        }
    }

    fn poll_events(&mut self, data: &mut StateData) -> PixEngineResult<()> {
        let turbo = self.turbo_clock < 3;
        self.clock_turbo(turbo);
        for event in data.poll() {
            match event {
                PixEvent::KeyPress(key, pressed, repeat) => {
                    self.handle_key_event(key, pressed, repeat, turbo, data)?
                }
                _ => (),
            }
        }
        Ok(())
    }

    fn clock_turbo(&mut self, turbo: bool) {
        let mut input = self.input.borrow_mut();
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

    fn handle_key_event(
        &mut self,
        key: Key,
        pressed: bool,
        repeat: bool,
        turbo: bool,
        data: &mut StateData,
    ) -> PixEngineResult<()> {
        // Keydown or Keyup
        match key {
            Key::Ctrl => self.ctrl = pressed,
            Key::LShift | Key::RShift => self.shift = pressed,
            _ if !self.ctrl && !self.shift => self.handle_input_event(key, pressed, turbo),
            _ => (),
        }

        if pressed {
            match key {
                // Debug =======================================================================
                Key::C if self.debug => {
                    let _ = self.console.clock();
                }
                Key::F if self.debug => self.console.clock_frame(),
                _ => (),
            }
            if !repeat {
                // Keydown
                if self.ctrl {
                    match key {
                        // UI ==========================================================================
                        Key::Return => {
                            self.settings.fullscreen = !self.settings.fullscreen;
                            data.fullscreen(self.settings.fullscreen);
                        }
                        Key::V if self.shift => {
                            self.settings.vsync = !self.settings.vsync;
                            data.vsync(self.settings.vsync);
                        }
                        Key::V if !self.shift => eprintln!("Recording not implemented"), // TODO
                        Key::M => self.settings.sound_enabled = !self.settings.sound_enabled,
                        // Open
                        Key::O => eprintln!("Open Dialog not implemented"), // TODO
                        // Reset
                        Key::R => {
                            self.running = true;
                            self.console.reset();
                        }
                        // Power Cycle
                        Key::P => {
                            self.running = true;
                            self.console.power_cycle();
                        }
                        // Change speed
                        Key::Minus => self.change_speed(-0.25),
                        Key::Equals => self.change_speed(0.25),
                        // Save/Load
                        Key::S if self.settings.save_enabled => {
                            self.console.save_state(self.settings.save_slot)?
                        }
                        Key::L if self.settings.save_enabled => {
                            self.console.load_state(self.settings.save_slot)?
                        }
                        Key::Num1 => self.settings.save_slot = 1,
                        Key::Num2 => self.settings.save_slot = 2,
                        Key::Num3 => self.settings.save_slot = 3,
                        Key::Num4 => self.settings.save_slot = 4,
                        // Debug =======================================================================
                        Key::D => {
                            let debug_width = 500;
                            let debug_height = self.height;
                            self.debug = !self.debug;
                            if self.debug {
                                self.width += debug_width;
                            } else {
                                self.width -= debug_width;
                            }
                            data.set_screen_size(self.width, self.height);
                            data.create_texture(
                                "cpu_debug",
                                ColorType::RGBA,
                                Rect::new(0, 0, debug_width, debug_height),
                                Rect::new(self.width - debug_width, 0, debug_width, debug_height),
                            );
                        }
                        _ => (),
                    }
                } else {
                    match key {
                        // UI ==========================================================================
                        Key::Escape => self.toggle_menu(), // TODO menu
                        // Fast-forward
                        Key::Space => {
                            self.settings.speed = 2.0;
                            self.console.set_speed(self.settings.speed);
                        }
                        // Utilities ===================================================================
                        Key::F9 => eprintln!("Toggle Logging Setting not implemented"), // TODO
                        Key::F10 => util::screenshot(&self.console.frame()),
                        _ => (),
                    }
                }
            }
        } else {
            // Keyup
            match key {
                Key::Space => {
                    self.settings.speed = DEFAULT_SPEED;
                    self.console.set_speed(self.settings.speed);
                }
                _ => (),
            }
        }
        Ok(())
    }

    fn handle_input_event(&mut self, key: Key, pressed: bool, turbo: bool) {
        let mut input = self.input.borrow_mut();
        match key {
            // Gamepad
            Key::Z => input.gamepad1.a = pressed,
            Key::X => input.gamepad1.b = pressed,
            Key::A => {
                input.gamepad1.turbo_a = pressed;
                input.gamepad1.a = turbo && pressed;
            }
            Key::S => {
                input.gamepad1.turbo_b = pressed;
                input.gamepad1.b = turbo && pressed;
            }
            Key::RShift => input.gamepad1.select = pressed,
            Key::Return => input.gamepad1.start = pressed,
            Key::Up => {
                if !self.settings.concurrent_dpad && pressed {
                    input.gamepad1.down = false;
                }
                input.gamepad1.up = pressed;
            }
            Key::Down => {
                if !self.settings.concurrent_dpad && pressed {
                    input.gamepad1.up = false;
                }
                input.gamepad1.down = pressed;
            }
            Key::Left => {
                if !self.settings.concurrent_dpad && pressed {
                    input.gamepad1.right = false;
                }
                input.gamepad1.left = pressed;
            }
            Key::Right => {
                if !self.settings.concurrent_dpad && pressed {
                    input.gamepad1.left = false;
                }
                input.gamepad1.right = pressed;
            }
            _ => (),
        }
    }

    fn toggle_menu(&mut self) {
        self.running = !self.running;
        // TODO menu overlay
    }

    fn change_speed(&mut self, delta: f64) {
        if self.settings.speed >= MIN_SPEED && self.settings.speed <= MAX_SPEED {
            self.settings.speed += DEFAULT_SPEED * delta;
            self.console.set_speed(self.settings.speed);
        }
    }

    fn update_title(&mut self, data: &mut StateData) {
        let mut title = DEFAULT_TITLE.to_string();
        if !self.running {
            title.push_str(" - Paused");
        } else {
            title.push_str(&format!(" - Save Slot: {}", self.settings.save_slot));
            if self.settings.speed != DEFAULT_SPEED {
                title.push_str(&format!(" - Speed: {}%", self.settings.speed * 100.0));
            }
        }
        data.set_title(&title);
    }

    fn draw_cpu_debug(&mut self, data: &mut StateData) {
        let x = 10;
        let y = 10;
        let wh = pixel::WHITE;

        data.set_draw_target(Sprite::new_rgba8(500, self.height));
        data.fill(pixel::VERY_DARK_GRAY);

        // CPU Status
        let cpu = &self.console.cpu;
        data.draw_string(x, y, "STATUS:", wh);
        let scolor = |f| {
            if cpu.status & f as u8 > 0 {
                pixel::RED
            } else {
                pixel::GREEN
            }
        };

        let ox = 8 * 16;
        data.draw_string(x + ox, y, "N", scolor(StatusRegs::N));
        data.draw_string(x + ox + 16, y, "V", scolor(StatusRegs::V));
        data.draw_string(x + ox + 32, y, "-", scolor(StatusRegs::U));
        data.draw_string(x + ox + 48, y, "B", scolor(StatusRegs::B));
        data.draw_string(x + ox + 64, y, "D", scolor(StatusRegs::D));
        data.draw_string(x + ox + 80, y, "I", scolor(StatusRegs::I));
        data.draw_string(x + ox + 96, y, "Z", scolor(StatusRegs::Z));
        data.draw_string(x + ox + 112, y, "C", scolor(StatusRegs::C));

        data.draw_string(x, y + 20, &format!("PC: ${:04X}", cpu.pc), wh);
        data.draw_string(x, y + 40, &format!("A: ${:02X}", cpu.acc), wh);
        data.draw_string(x, y + 80, &format!("X: ${:02X}", cpu.x), wh);
        data.draw_string(x, y + 100, &format!("Y: ${:02X}", cpu.y), wh);
        data.draw_string(x, y + 120, &format!("SP: ${:04X}", cpu.sp), wh);
        let pixels = &data.get_draw_target().raw_pixels();
        data.copy_texture("cpu_debug", pixels);
    }
}

impl State for Ui {
    fn on_start(&mut self, data: &mut StateData) -> PixEngineResult<()> {
        // TODO fix current_dir to be argument passed
        // self.current_dir = PathBuf::from("roms/legend_of_zelda.nes");
        self.current_dir = PathBuf::from("roms/castlevania_iii_draculas_curse.nes");
        let mut roms = util::find_roms(&self.current_dir)?;
        self.roms.append(&mut roms);
        if self.roms.len() == 1 {
            self.loaded_rom = self.roms[0].clone();
            self.console.load_rom(&self.loaded_rom)?;
            self.console.power_on()?;
            if self.settings.save_enabled {
                self.console.load_state(self.settings.save_slot)?;
            }
            self.running = true;
        }

        data.create_texture(
            "nes",
            ColorType::RGB,
            Rect::new(0, 8, RENDER_WIDTH, RENDER_HEIGHT - 16), // Trims overscan
            Rect::new(0, 0, self.width, self.height),
        );

        // Smooths out startup graphic glitches for some games
        if self.running {
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

    fn on_update(&mut self, _elapsed: Duration, data: &mut StateData) -> PixEngineResult<()> {
        self.poll_events(data)?;
        self.update_title(data);

        if self.running {
            // Clock NES
            for _ in 0..self.settings.speed as usize {
                self.console.clock_frame();
                self.turbo_clock = (1 + self.turbo_clock) % 6;
            }
        }

        // Update screen
        data.copy_texture("nes", &self.console.frame());
        if self.debug {
            self.draw_cpu_debug(data);
        }

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
