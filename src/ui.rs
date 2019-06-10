//! User Interface around the NES Console

use crate::console::Console;
use crate::input::{Input, InputRef};
use crate::ui::window::Window;
use crate::util::{self, Result};
use sdl2::controller::Axis;
use sdl2::controller::{Button, GameController};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::EventPump;
use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

mod window;

const DEFAULT_TITLE: &str = "RustyNES";
const DEFAULT_SPEED: f64 = 100.0; // 100% - 60 Hz
const MIN_SPEED: f64 = 25.0; // 25% - 240 Hz
const MAX_SPEED: f64 = 200.0; // 200% - 30 Hz
const GAMEPAD_AXIS_DEADZONE: i16 = 8000;

/// User Interface builder for UiState
#[derive(Default)]
pub struct UiBuilder {
    path: PathBuf,
    debug: bool,
    ppu_debug: bool,
    fullscreen: bool,
    sound_off: bool,
    concurrent_dpad: bool,
    randomize_ram: bool,
    log_cpu: bool,
    no_save: bool,
    save_slot: u8,
    scale: u32,
}

impl UiBuilder {
    pub fn new() -> Self {
        Self {
            path: PathBuf::new(),
            debug: false,
            ppu_debug: false,
            fullscreen: false,
            sound_off: false,
            concurrent_dpad: false,
            randomize_ram: false,
            log_cpu: false,
            no_save: false,
            save_slot: 1u8,
            scale: 1u32,
        }
    }

    pub fn path(&mut self, path: Option<PathBuf>) -> &mut Self {
        self.path = path.unwrap_or_else(|| env::current_dir().unwrap_or_default());
        self
    }
    pub fn debug(&mut self, val: bool) -> &mut Self {
        self.debug = val;
        self
    }
    pub fn ppu_debug(&mut self, val: bool) -> &mut Self {
        self.ppu_debug = val;
        self
    }
    pub fn fullscreen(&mut self, val: bool) -> &mut Self {
        self.fullscreen = val;
        self
    }
    pub fn sound_off(&mut self, val: bool) -> &mut Self {
        self.sound_off = val;
        self
    }
    pub fn concurrent_dpad(&mut self, val: bool) -> &mut Self {
        self.concurrent_dpad = val;
        self
    }
    pub fn randomize_ram(&mut self, val: bool) -> &mut Self {
        self.randomize_ram = val;
        self
    }
    pub fn log_cpu(&mut self, val: bool) -> &mut Self {
        self.log_cpu = val;
        self
    }
    pub fn no_save(&mut self, val: bool) -> &mut Self {
        self.no_save = val;
        self
    }
    pub fn save_slot(&mut self, val: u8) -> &mut Self {
        self.save_slot = val;
        self
    }
    pub fn scale(&mut self, val: u32) -> &mut Self {
        self.scale = val;
        self
    }
    pub fn build(&self) -> Result<Ui> {
        let input = Rc::new(RefCell::new(Input::new()));
        let mut console = Console::init(input.clone());
        console.debug(self.debug);
        console.log_cpu(self.log_cpu);
        console.no_save(self.no_save);
        console.randomize_ram(self.randomize_ram);

        let (window, event_pump) =
            Window::init(DEFAULT_TITLE, self.scale, self.fullscreen, self.ppu_debug)?;
        Ok(Ui {
            path: self.path.clone(),
            roms: Vec::new(),
            ppu_debug: self.ppu_debug,
            paused: false,
            fullscreen: self.fullscreen,
            should_close: false,
            sound_enabled: !self.sound_off,
            concurrent_dpad: self.concurrent_dpad,
            fastforward: false,
            lctrl: false,
            save_slot: 1u8,
            turbo_clock: 0u8,
            avg_fps: Duration::from_millis(60),
            past_fps: [Duration::from_millis(60); 20],
            speed: DEFAULT_SPEED,
            speed_counter: 0i32,
            console,
            window,
            event_pump: RefCell::new(event_pump),
            input,
            gamepad1: None,
            gamepad2: None,
        })
    }
}

pub struct Ui {
    path: PathBuf,
    roms: Vec<PathBuf>,
    ppu_debug: bool,
    paused: bool,
    fullscreen: bool,
    should_close: bool,
    fastforward: bool,
    sound_enabled: bool,
    concurrent_dpad: bool,
    lctrl: bool,
    save_slot: u8,
    turbo_clock: u8,
    avg_fps: Duration,
    past_fps: [Duration; 20], // Running total of last X frames to avoid value jitter
    speed: f64,
    speed_counter: i32,
    console: Console,
    window: Window,
    event_pump: RefCell<EventPump>,
    input: InputRef,
    gamepad1: Option<GameController>,
    gamepad2: Option<GameController>,
}

impl Ui {
    pub fn run(&mut self) -> Result<()> {
        self.update_title()?;

        let mut roms = util::find_roms(&self.path)?;
        self.roms.append(&mut roms);

        if self.roms.len() == 1 {
            self.console.load_rom(&self.roms[0])?;
            self.console.power_on()?;
            self.console.load_state(self.save_slot)?;
            if self.ppu_debug {
                self.window.set_debug_size()?;
            }
        }

        let startup_frames = 40;
        for _ in 0..startup_frames {
            self.poll_events()?;
            self.console.clock_frame();
            self.window.render_blank()?;
            let samples = self.console.audio_samples();
            if self.sound_enabled {
                self.window.enqueue_audio(&samples);
            }
            samples.clear();
        }

        let mut start = Instant::now();
        let mut fps_frame = 0;
        let one_sec = Duration::from_secs(1);
        while !self.should_close {
            self.poll_events()?;
            if !self.paused {
                // Frames that aren't multiples of the default render 1 more/less
                // frames every other frame
                let mut frames_to_run = 0;
                self.speed_counter += self.speed as i32;
                while self.speed_counter > 0 {
                    self.speed_counter -= 100;
                    frames_to_run += 1;
                }
                for _ in 0..frames_to_run {
                    self.console.clock_frame();
                    self.turbo_clock = (1 + self.turbo_clock) % 6;
                }
                let game_view = self.console.frame();
                if self.ppu_debug {
                    let nametables = self.console.nametables();
                    let pattern_tables = self.console.pattern_tables();
                    let palettes = self.console.palettes();
                    self.window
                        .update_debug(game_view, nametables, pattern_tables, palettes)?;
                } else {
                    self.window.update_frame(game_view)?;
                }

                if self.sound_enabled {
                    let samples = self.console.audio_samples();
                    self.window.enqueue_audio(&samples);
                    samples.clear();
                } else {
                    self.console.audio_samples().clear();
                }
                let end = Instant::now();

                fps_frame += 1;
                let delta = (end - start).as_millis() as u32;
                self.past_fps[fps_frame % 20] = one_sec.checked_div(delta).unwrap();

                for fps in self.past_fps.iter() {
                    self.avg_fps += *fps;
                }
                self.avg_fps /= 20;
                self.update_title()?;
                start = end;
            }
        }

        self.console.power_off()
    }

    pub fn poll_events(&mut self) -> Result<()> {
        let turbo = self.turbo_clock < 3;
        // Toggle turbo every poll as long as turbo button is held down
        self.clock_turbo(turbo);
        let events: Vec<Event> = {
            let mut event_pump = self.event_pump.borrow_mut();
            event_pump.poll_iter().collect()
        };
        for event in events {
            match event {
                Event::ControllerDeviceAdded { which: id, .. } => {
                    eprintln!("Gamepad {} connected.", id);
                    match id {
                        0 => self.gamepad1 = Some(self.window.controller_sub.open(id)?),
                        1 => self.gamepad2 = Some(self.window.controller_sub.open(id)?),
                        _ => (),
                    }
                }
                Event::Quit { .. } | Event::AppTerminating { .. } => self.should_close = true,
                Event::Window { win_event, .. } => match win_event {
                    WindowEvent::Resized(..) | WindowEvent::SizeChanged(..) => {
                        if self.ppu_debug {
                            self.window.render_debug()?
                        } else {
                            self.window.render_frame()?
                        }
                    }
                    _ => (),
                },
                Event::KeyDown {
                    keycode: Some(key),
                    repeat,
                    ..
                } => {
                    if !repeat {
                        self.handle_keydown(key, turbo)?;
                    }
                }
                Event::KeyUp {
                    keycode: Some(key),
                    repeat,
                    ..
                } => {
                    if !repeat {
                        match key {
                            Keycode::Space => self.set_fastforward(false)?,
                            Keycode::LCtrl => self.lctrl = false,
                            _ => self.handle_keyboard_event(key, false, turbo),
                        }
                    }
                }
                Event::ControllerButtonDown { which, button, .. } => match button {
                    Button::LeftStick => self.toggle_menu()?,
                    Button::RightStick => self.set_fastforward(true)?,
                    Button::LeftShoulder => self.console.save_state(self.save_slot)?,
                    Button::RightShoulder => self.console.load_state(self.save_slot)?,
                    _ => self.handle_gamepad_button(which, button, true, turbo),
                },
                Event::ControllerButtonUp { which, button, .. } => match button {
                    Button::RightStick => self.set_fastforward(false)?,
                    _ => self.handle_gamepad_button(which, button, false, turbo),
                },
                Event::ControllerAxisMotion {
                    which, axis, value, ..
                } => self.handle_gamepad_axis(which, axis, value, turbo),
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

    fn handle_keydown(&mut self, key: Keycode, turbo: bool) -> Result<()> {
        match key {
            Keycode::Escape => self.toggle_menu()?,
            Keycode::LCtrl => self.lctrl = true,
            Keycode::O if self.lctrl => eprintln!("Open not implemented"), // TODO
            Keycode::Q if self.lctrl => self.should_close = true,
            Keycode::R if self.lctrl => self.console.reset(),
            Keycode::P if self.lctrl => self.console.power_cycle(),
            Keycode::Equals if self.lctrl => self.change_speed(25.0)?,
            Keycode::Minus if self.lctrl => self.change_speed(-25.0)?,
            Keycode::Space => self.set_fastforward(true)?,
            Keycode::Num1 if self.lctrl => {
                self.save_slot = 1;
                self.update_title()?;
            }
            Keycode::Num2 if self.lctrl => {
                self.save_slot = 2;
                self.update_title()?;
            }
            Keycode::Num3 if self.lctrl => {
                self.save_slot = 3;
                self.update_title()?;
            }
            Keycode::Num4 if self.lctrl => {
                self.save_slot = 4;
                self.update_title()?;
            }
            Keycode::S if self.lctrl => self.console.save_state(self.save_slot)?,
            Keycode::L if self.lctrl => self.console.load_state(self.save_slot)?,
            Keycode::M if self.lctrl => self.sound_enabled = !self.sound_enabled,
            Keycode::V if self.lctrl => eprintln!("Recording not implemented"), // TODO
            Keycode::D if self.lctrl => {
                if !self.fullscreen {
                    self.console.debug(true);
                }
            }
            Keycode::Return if self.lctrl => {
                self.fullscreen = !self.fullscreen;
                self.window.toggle_fullscreen()?;
            }
            Keycode::F10 => util::screenshot(&self.console.frame()),
            Keycode::F9 => eprintln!("Logging not implemented"), // TODO
            _ => self.handle_keyboard_event(key, true, turbo),
        }
        Ok(())
    }

    fn change_speed(&mut self, delta: f64) -> Result<()> {
        if delta > 0.0 && self.speed < MAX_SPEED {
            self.speed_counter = 0;
            self.speed += 25.0;
        } else if delta < 0.0 && self.speed > MIN_SPEED {
            self.speed_counter = 0;
            self.speed -= 25.0;
        }
        self.console.set_speed(self.speed / DEFAULT_SPEED);
        self.update_title()
    }

    fn update_title(&mut self) -> Result<()> {
        let mut title = DEFAULT_TITLE.to_string();
        if self.paused {
            title.push_str(" - Paused");
        } else {
            title.push_str(&format!(
                " - FPS: {} - Save Slot: {}",
                self.avg_fps.as_millis(),
                self.save_slot
            ));
            if self.speed != DEFAULT_SPEED {
                title.push_str(&format!(" - Speed: {}%", self.speed));
            }
        }
        self.window.set_title(&title)
    }

    fn toggle_menu(&mut self) -> Result<()> {
        self.paused = !self.paused;
        self.update_title()
        // TODO menu overlay
    }

    fn set_fastforward(&mut self, val: bool) -> Result<()> {
        self.speed_counter = 0;
        let old_fastforward = self.fastforward;
        self.fastforward = val;
        if old_fastforward != self.fastforward {
            if self.fastforward {
                self.speed = MAX_SPEED;
            } else {
                self.speed = DEFAULT_SPEED;
            }
            self.console.set_speed(self.speed / DEFAULT_SPEED);
            self.update_title()?;
        }
        Ok(())
    }

    fn handle_keyboard_event(&mut self, key: Keycode, down: bool, turbo: bool) {
        let mut input = self.input.borrow_mut();
        match key {
            Keycode::Z => input.gamepad1.a = down,
            Keycode::X => input.gamepad1.b = down,
            Keycode::A => {
                input.gamepad1.turbo_a = down;
                input.gamepad1.a = turbo && down;
            }
            Keycode::S => {
                input.gamepad1.turbo_b = down;
                input.gamepad1.b = turbo && down;
            }
            Keycode::RShift => input.gamepad1.select = down,
            Keycode::Return => input.gamepad1.start = down,
            Keycode::Up => {
                if !self.concurrent_dpad && down {
                    input.gamepad1.down = false;
                }
                input.gamepad1.up = down;
            }
            Keycode::Down => {
                if !self.concurrent_dpad && down {
                    input.gamepad1.up = false;
                }
                input.gamepad1.down = down;
            }
            Keycode::Left => {
                if !self.concurrent_dpad && down {
                    input.gamepad1.right = false;
                }
                input.gamepad1.left = down;
            }
            Keycode::Right => {
                if !self.concurrent_dpad && down {
                    input.gamepad1.left = false;
                }
                input.gamepad1.right = down;
            }
            _ => {}
        }
    }

    fn handle_gamepad_button(&mut self, gamepad_id: i32, button: Button, down: bool, turbo: bool) {
        let mut input = self.input.borrow_mut();
        let mut gamepad = match gamepad_id {
            0 => &mut input.gamepad1,
            1 => &mut input.gamepad2,
            _ => panic!("invalid gamepad id: {}", gamepad_id),
        };
        match button {
            Button::A => {
                gamepad.a = down;
            }
            Button::B => gamepad.b = down,
            Button::X => {
                gamepad.turbo_a = down;
                gamepad.a = turbo && down;
            }
            Button::Y => {
                gamepad.turbo_b = down;
                gamepad.b = turbo && down;
            }
            Button::Back => gamepad.select = down,
            Button::Start => gamepad.start = down,
            Button::DPadUp => gamepad.up = down,
            Button::DPadDown => gamepad.down = down,
            Button::DPadLeft => gamepad.left = down,
            Button::DPadRight => gamepad.right = down,
            _ => {}
        }
    }

    fn handle_gamepad_axis(&mut self, gamepad_id: i32, axis: Axis, value: i16, turbo: bool) {
        let mut input = self.input.borrow_mut();
        let mut gamepad = match gamepad_id {
            0 => &mut input.gamepad1,
            1 => &mut input.gamepad2,
            _ => panic!("invalid gamepad id: {}", gamepad_id),
        };
        match axis {
            // Left/Right
            Axis::LeftX => {
                if value < -GAMEPAD_AXIS_DEADZONE {
                    gamepad.left = true;
                } else if value > GAMEPAD_AXIS_DEADZONE {
                    gamepad.right = true;
                } else {
                    gamepad.left = false;
                    gamepad.right = false;
                }
            }
            // Down/Up
            Axis::LeftY => {
                if value < -GAMEPAD_AXIS_DEADZONE {
                    gamepad.up = true;
                } else if value > GAMEPAD_AXIS_DEADZONE {
                    gamepad.down = true;
                } else {
                    gamepad.up = false;
                    gamepad.down = false;
                }
            }
            Axis::TriggerLeft => {
                if value > GAMEPAD_AXIS_DEADZONE {
                    gamepad.turbo_a = true;
                    gamepad.a = turbo;
                } else {
                    gamepad.turbo_a = false;
                    gamepad.a = false;
                }
            }
            Axis::TriggerRight => {
                if value > GAMEPAD_AXIS_DEADZONE {
                    gamepad.turbo_b = true;
                    gamepad.b = turbo;
                } else {
                    gamepad.turbo_b = false;
                    gamepad.b = false;
                }
            }
            _ => (),
        }
    }
}
