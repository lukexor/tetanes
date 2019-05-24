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

const DEFAULT_FRAME_RATE: f64 = 60.0; // 60 Hz
const MIN_FRAME_RATE: f64 = 15.0;
const MAX_FRAME_RATE: f64 = 240.0;

pub struct UI {
    roms: Vec<PathBuf>,
    window: Window,
    frame_rate: f64,
    debug: bool,
    paused: bool,
    sound_enabled: bool,
}

impl UI {
    pub fn init(roms: Vec<PathBuf>, scale: u32, debug: bool) -> Result<Self> {
        if roms.is_empty() {
            Err(format_err!("no rom files found or specified"))?;
        }
        Ok(Self {
            roms,
            window: Window::with_scale(scale)?,
            frame_rate: DEFAULT_FRAME_RATE,
            debug,
            paused: false,
            sound_enabled: true,
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
                console.step_frame(self.frame_rate);
                self.window.render(&console.render());
            }
            if self.sound_enabled && self.frame_rate == DEFAULT_FRAME_RATE {
                self.window.enqueue_audio(&mut console.audio_samples());
            } else {
                console.audio_samples().clear();
            }

            match console.poll_events() {
                Continue => (),
                Quit => break,
                Menu => self.paused = !self.paused,
                Reset => console.reset(),
                PowerCycle => console.power_cycle(),
                IncSpeed => {
                    if self.frame_rate < MAX_FRAME_RATE {
                        self.frame_rate += 0.25 * DEFAULT_FRAME_RATE;
                    }
                }
                DecSpeed => {
                    if self.frame_rate > MIN_FRAME_RATE {
                        self.frame_rate -= 0.25 * DEFAULT_FRAME_RATE;
                    }
                }
                FastForward(toggle) => {
                    if toggle {
                        self.frame_rate = MIN_FRAME_RATE;
                    } else {
                        self.frame_rate = DEFAULT_FRAME_RATE;
                    }
                }
                Save(slot) => eprintln!("Save {} not implemented", slot), // TODO
                Load(slot) => eprintln!("Load {} not implemented", slot), // TODO
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
