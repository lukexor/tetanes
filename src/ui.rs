pub use sdl2::EventPump;

use crate::console::Console;
use crate::input::{Input, InputResult::*};
use crate::ui::window::Window;
use crate::util::Result;
use failure::format_err;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

mod window;

const DEFAULT_SPEED: f64 = 100.0; // 100% - 60 Hz
const MIN_SPEED: f64 = 25.0; // 25% - 240 Hz
const MAX_SPEED: f64 = 200.0; // 200% - 30 Hz

pub struct UI {
    roms: Vec<PathBuf>,
    window: Window,
    speed: f64,
    debug: bool,
    fastforward: bool,
    paused: bool,
    sound_enabled: bool,
    state_slot: u8,
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
        })
    }

    pub fn run(&mut self) -> Result<()> {
        if self.roms.len() == 1 {
            let rom = self.roms[0].clone();
            self.play_game(rom)?;
        } else {
            // TODO Menu view
        };
        Ok(())
    }

    pub fn play_game(&mut self, rom: PathBuf) -> Result<()> {
        let event_pump = self.window.event_pump.take().unwrap();
        let input = Rc::new(RefCell::new(Input::init(event_pump)));
        let mut console = Console::power_on(rom, input.clone())?;
        console.debug(self.debug);
        loop {
            if !self.paused {
                let mut frames_to_run = (self.speed / DEFAULT_SPEED).floor() as usize;
                if frames_to_run == 0 {
                    frames_to_run = 1;
                }
                for _ in 0..frames_to_run {
                    console.step_frame();
                }
                self.window.render(&console.render());

                if self.sound_enabled {
                    self.window.enqueue_audio(&mut console.audio_samples());
                } else {
                    console.audio_samples().clear();
                }
            }

            match console.poll_events() {
                Continue => (),
                Quit => break,
                Open => eprintln!("Open not implemented"), // TODO
                Menu => self.paused = !self.paused,
                Reset => console.reset(),
                PowerCycle => console.power_cycle(),
                IncSpeed => {
                    if self.speed < MAX_SPEED {
                        self.speed += 25.0;
                        console.set_speed(self.speed / DEFAULT_SPEED);
                    }
                }
                DecSpeed => {
                    if self.speed > MIN_SPEED {
                        self.speed -= 25.0;
                        console.set_speed(self.speed / DEFAULT_SPEED);
                    }
                }
                FastForward => {
                    self.fastforward = !self.fastforward;
                    if self.fastforward {
                        self.speed = MAX_SPEED;
                    } else {
                        self.speed = DEFAULT_SPEED;
                    }
                    console.set_speed(self.speed / DEFAULT_SPEED);
                }
                SetState(slot) => self.state_slot = slot,
                Save => console.save_state(self.state_slot)?,
                Load => console.load_state(self.state_slot)?,
                ToggleSound => self.sound_enabled = !self.sound_enabled,
                ToggleFullscreen => self.window.toggle_fullscreen(),
                ToggleDebug => {
                    self.debug = !self.debug;
                    console.debug(self.debug)
                }
                Screenshot => crate::util::screenshot(&console.render()),
                ToggleRecord => eprintln!("Recording not implemented"), // TODO
                CycleLogLevel => eprintln!("Logging not implemented"),  // TODO
            }
        }
        console.power_off()?;
        Ok(())
    }
}
