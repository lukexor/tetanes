//! User Interface around the NES Console

use crate::console::Console;
use crate::input::{Input, InputRef};
use crate::ui::window::Window;
use crate::util::Result;
use failure::format_err;
use sdl2::controller::Axis;
use sdl2::controller::{Button, GameController};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::EventPump;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

mod window;

const DEFAULT_SPEED: f64 = 100.0; // 100% - 60 Hz
const MIN_SPEED: f64 = 25.0; // 25% - 240 Hz
const MAX_SPEED: f64 = 200.0; // 200% - 30 Hz
const CONTROLLER_AXIS_DEADZONE: i16 = 8000;

/// User Interface
pub struct UI {
    roms: Vec<PathBuf>,
    speed: f64,
    debug: bool,
    fastforward: bool,
    paused: bool,
    sound_enabled: bool,
    state_slot: u8,
    lctrl: bool,
    controller1: Option<GameController>,
    controller2: Option<GameController>,
}

impl UI {
    pub fn init(roms: Vec<PathBuf>, debug: bool) -> Result<Self> {
        if roms.is_empty() {
            Err(format_err!("no rom files found or specified"))?;
        }
        Ok(Self {
            roms,
            speed: DEFAULT_SPEED,
            debug,
            fastforward: false,
            paused: false,
            sound_enabled: true,
            state_slot: 1u8,
            lctrl: false,
            controller1: None,
            controller2: None,
        })
    }

    pub fn run(&mut self, fullscreen: bool, scale: u32, load_slot: Option<u8>) -> Result<()> {
        let (mut window, mut event_pump) = Window::with_scale(scale)?;

        if fullscreen {
            window.toggle_fullscreen();
        }

        if let Some(slot) = load_slot {
            self.state_slot = slot;
        }

        if self.roms.len() == 1 {
            let rom = self.roms[0].clone();
            self.play_game(&mut window, &mut event_pump, rom, load_slot)?;
        } else {
            // TODO Menu view
        };

        Ok(())
    }

    pub fn play_game(
        &mut self,
        window: &mut Window,
        event_pump: &mut EventPump,
        rom: PathBuf,
        load_slot: Option<u8>,
    ) -> Result<()> {
        let input = Rc::new(RefCell::new(Input::new()));
        let mut console = Console::power_on(rom, input.clone())?;

        console.debug(self.debug);
        if let Some(slot) = load_slot {
            console.load_state(slot)?;
        }

        if let Ok(count) = window.controller_sub.num_joysticks() {
            if count > 0 && window.controller_sub.is_game_controller(0) {
                self.controller1 = Some(window.controller_sub.open(0)?);
                if count > 1 && window.controller_sub.is_game_controller(1) {
                    self.controller2 = Some(window.controller_sub.open(1)?);
                }
            }
        }

        loop {
            self.poll_events(window, event_pump, &mut console, &input)?;
            if !self.paused {
                let mut frames_to_run = (self.speed / DEFAULT_SPEED).floor() as usize;
                if frames_to_run == 0 {
                    frames_to_run = 1;
                }
                for _ in 0..frames_to_run {
                    console.clock_frame();
                }
                window.render(&console.render());

                if self.sound_enabled {
                    window.enqueue_audio(&mut console.audio_samples());
                } else {
                    console.audio_samples().clear();
                }
            }
        }
    }

    pub fn poll_events(
        &mut self,
        window: &mut Window,
        event_pump: &mut EventPump,
        console: &mut Console,
        input: &InputRef,
    ) -> Result<()> {
        let turbo = console.cpu.mem.ppu.frame() % 6 < 3;
        {
            let mut input = input.borrow_mut();
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
        for event in event_pump.poll_iter() {
            match event {
                Event::ControllerDeviceAdded { which: id, .. } => {
                    eprintln!("Controller {} connected.", id);
                    match id {
                        0 => self.controller1 = Some(window.controller_sub.open(id)?),
                        1 => self.controller2 = Some(window.controller_sub.open(id)?),
                        _ => (),
                    }
                }
                Event::Quit { .. } => std::process::exit(0),
                Event::KeyDown {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::Escape => self.toggle_menu(),
                    Keycode::LCtrl => self.lctrl = true,
                    Keycode::O if self.lctrl => eprintln!("Open not implemented"), // TODO
                    Keycode::Q if self.lctrl => std::process::exit(0),
                    Keycode::R if self.lctrl => console.reset(),
                    Keycode::P if self.lctrl => console.power_cycle(),
                    Keycode::Equals if self.lctrl => {
                        if self.speed < MAX_SPEED {
                            self.speed += 25.0;
                            console.set_speed(self.speed / DEFAULT_SPEED);
                        }
                    }
                    Keycode::Minus if self.lctrl => {
                        if self.speed > MIN_SPEED {
                            self.speed -= 25.0;
                            console.set_speed(self.speed / DEFAULT_SPEED);
                        }
                    }
                    Keycode::Space => self.toggle_fastforward(console),
                    Keycode::Num1 if self.lctrl => self.state_slot = 1,
                    Keycode::Num2 if self.lctrl => self.state_slot = 2,
                    Keycode::Num3 if self.lctrl => self.state_slot = 2,
                    Keycode::Num4 if self.lctrl => self.state_slot = 3,
                    Keycode::S if self.lctrl => console.save_state(self.state_slot)?,
                    Keycode::L if self.lctrl => console.load_state(self.state_slot)?,
                    Keycode::M if self.lctrl => self.sound_enabled = !self.sound_enabled,
                    Keycode::V if self.lctrl => eprintln!("Recording not implemented"), // TODO
                    Keycode::D if self.lctrl => {
                        self.debug = !self.debug;
                        console.debug(self.debug);
                    }
                    Keycode::Return if self.lctrl => window.toggle_fullscreen(),
                    Keycode::F10 => crate::util::screenshot(&console.render()),
                    Keycode::F9 => eprintln!("Logging not implemented"), // TODO
                    _ => self.handle_keyboard_event(&input, key, true, turbo),
                },
                Event::KeyUp {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::LCtrl => self.lctrl = false,
                    _ => self.handle_keyboard_event(&input, key, false, turbo),
                },
                Event::ControllerButtonDown { which, button, .. } => match button {
                    Button::LeftStick => self.toggle_menu(),
                    Button::RightStick => self.toggle_fastforward(console),
                    Button::LeftShoulder => console.save_state(self.state_slot)?,
                    Button::RightShoulder => console.load_state(self.state_slot)?,
                    _ => self.handle_controller_event(&input, which, button, true, turbo),
                },
                Event::ControllerButtonUp { which, button, .. } => {
                    self.handle_controller_event(&input, which, button, false, turbo)
                }
                Event::ControllerAxisMotion {
                    which, axis, value, ..
                } => {
                    let mut input = input.borrow_mut();
                    let mut gamepad = match which {
                        0 => &mut input.gamepad1,
                        1 => &mut input.gamepad2,
                        _ => panic!("invalid controller id: {}", which),
                    };
                    match axis {
                        // Left/Right
                        Axis::LeftX => {
                            if value < -CONTROLLER_AXIS_DEADZONE {
                                gamepad.left = true;
                            } else if value > CONTROLLER_AXIS_DEADZONE {
                                gamepad.right = true;
                            } else {
                                gamepad.left = false;
                                gamepad.right = false;
                            }
                        }
                        // Down/Up
                        Axis::LeftY => {
                            if value < -CONTROLLER_AXIS_DEADZONE {
                                gamepad.up = true;
                            } else if value > CONTROLLER_AXIS_DEADZONE {
                                gamepad.down = true;
                            } else {
                                gamepad.up = false;
                                gamepad.down = false;
                            }
                        }
                        _ => (),
                    }
                }
                _ => (),
            }
        }
        Ok(())
    }

    fn toggle_menu(&mut self) {
        self.paused = !self.paused;
        // TODO menu overlay
    }

    fn toggle_fastforward(&mut self, console: &mut Console) {
        self.fastforward = !self.fastforward;
        if self.fastforward {
            self.speed = MAX_SPEED;
        } else {
            self.speed = DEFAULT_SPEED;
        }
        console.set_speed(self.speed / DEFAULT_SPEED);
    }

    fn handle_keyboard_event(&mut self, input: &InputRef, key: Keycode, down: bool, turbo: bool) {
        let mut input = input.borrow_mut();
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
            Keycode::Up => input.gamepad1.up = down,
            Keycode::Down => input.gamepad1.down = down,
            Keycode::Left => input.gamepad1.left = down,
            Keycode::Right => input.gamepad1.right = down,
            _ => {}
        }
    }

    fn handle_controller_event(
        &mut self,
        input: &InputRef,
        controller_id: i32,
        button: Button,
        down: bool,
        turbo: bool,
    ) {
        let mut input = input.borrow_mut();
        let mut gamepad = match controller_id {
            0 => &mut input.gamepad1,
            1 => &mut input.gamepad2,
            _ => panic!("invalid controller id: {}", controller_id),
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
}
