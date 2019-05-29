//! User Interface around the NES Console

use crate::console::Console;
use crate::input::{Input, InputRef};
use crate::ui::window::Window;
use crate::util::Result;
use failure::format_err;
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

/// User Interface
pub struct UI {
    roms: Vec<PathBuf>,
    window: Window,
    speed: f64,
    debug: bool,
    fastforward: bool,
    paused: bool,
    sound_enabled: bool,
    state_slot: u8,
    lctrl: bool,
}

impl UI {
    pub fn init(roms: Vec<PathBuf>, scale: u32, debug: bool) -> Result<Self> {
        if roms.is_empty() {
            Err(format_err!("no rom files found or specified"))?;
        }
        Ok(Self {
            roms,
            window: Window::with_scale(scale)?,
            speed: DEFAULT_SPEED,
            debug,
            fastforward: false,
            paused: false,
            sound_enabled: true,
            state_slot: 1u8,
            lctrl: false,
        })
    }

    pub fn run(&mut self, fullscreen: bool, load_slot: Option<u8>) -> Result<()> {
        if let Some(slot) = load_slot {
            self.state_slot = slot;
        }
        if self.roms.len() == 1 {
            let rom = self.roms[0].clone();
            self.play_game(rom, fullscreen, load_slot)?;
        } else {
            // TODO Menu view
        };
        Ok(())
    }

    pub fn play_game(
        &mut self,
        rom: PathBuf,
        fullscreen: bool,
        load_slot: Option<u8>,
    ) -> Result<()> {
        let mut event_pump = self.window.event_pump.take().unwrap();
        let input = Rc::new(RefCell::new(Input::new()));
        let mut console = Console::power_on(rom, input.clone())?;
        console.debug(self.debug);
        if fullscreen {
            self.window.toggle_fullscreen();
        }
        if let Some(slot) = load_slot {
            console.load_state(slot)?;
        }
        loop {
            let should_break = self.poll_events(&mut console, &input, &mut event_pump)?;
            if should_break {
                break;
            }
            if !self.paused {
                let mut frames_to_run = (self.speed / DEFAULT_SPEED).floor() as usize;
                if frames_to_run == 0 {
                    frames_to_run = 1;
                }
                for _ in 0..frames_to_run {
                    console.clock_frame();
                }
                self.window.render(&console.render());

                if self.sound_enabled {
                    self.window.enqueue_audio(&mut console.audio_samples());
                } else {
                    console.audio_samples().clear();
                }
            }
        }

        console.power_off()?;
        Ok(())
    }

    fn poll_events(
        &mut self,
        console: &mut Console,
        input: &InputRef,
        event_pump: &mut EventPump,
    ) -> Result<bool> {
        let turbo = console.cpu.mem.ppu.frame() % 6 < 3;
        {
            let mut input = input.borrow_mut();
            if input.turboa {
                input.gamepad1.a = turbo;
            }
            if input.turbob {
                input.gamepad1.b = turbo;
            }
        }
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => return Ok(true),
                Event::KeyDown {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::Escape => self.paused = !self.paused,
                    Keycode::LCtrl => self.lctrl = true,
                    Keycode::O if self.lctrl => eprintln!("Open not implemented"), // TODO
                    Keycode::Q if self.lctrl => return Ok(true),
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
                    Keycode::Space => {
                        self.fastforward = !self.fastforward;
                        if self.fastforward {
                            self.speed = MAX_SPEED;
                        } else {
                            self.speed = DEFAULT_SPEED;
                        }
                        console.set_speed(self.speed / DEFAULT_SPEED);
                    }
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
                    Keycode::Return if self.lctrl => self.window.toggle_fullscreen(),
                    Keycode::F10 => crate::util::screenshot(&console.render()),
                    Keycode::F9 => eprintln!("Logging not implemented"), // TODO
                    _ => self.handle_gamepad_event(&input, key, true, turbo),
                },
                Event::KeyUp {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::LCtrl => self.lctrl = false,
                    _ => self.handle_gamepad_event(&input, key, false, turbo),
                },
                _ => (),
            }
        }
        Ok(false)
    }

    fn handle_gamepad_event(&mut self, input: &InputRef, key: Keycode, down: bool, turbo: bool) {
        let mut input = input.borrow_mut();
        match key {
            Keycode::Z => input.gamepad1.a = down,
            Keycode::X => input.gamepad1.b = down,
            Keycode::A => {
                input.turboa = down;
                input.gamepad1.a = turbo && down;
            }
            Keycode::S => {
                input.turbob = down;
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
}
