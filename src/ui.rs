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

const DEFAULT_SPEED: u16 = 60; // 60 Hz

pub struct UI {
    roms: Vec<PathBuf>,
    window: Window,
    speed: u16,
    fastforward: bool,
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
            speed: DEFAULT_SPEED,
            fastforward: false,
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
                console.step_frame(self.speed);
                self.window.render(&console.render());
            }
            if self.sound_enabled {
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
                IncSpeed => eprintln!("Increase speed not implemented"), // TODO
                DecSpeed => eprintln!("Decrease speed not implemented"), // TODO
                FastForward => self.fastforward = !self.fastforward,     // TODO
                Save(slot) => eprintln!("Save {} not implemented", slot), // TODO
                Load(slot) => eprintln!("Load {} not implemented", slot), // TODO
                ToggleSound => self.sound_enabled = !self.sound_enabled,
                ToggleFullscreen => self.window.toggle_fullscreen(),
                ToggleDebug => {
                    self.debug = !self.debug;
                    console.debug(self.debug)
                }
                Screenshot => eprintln!("Screenshot not implemented"), // TODO,
                ToggleRecord => eprintln!("Recording not implemented"), // TODO,
            }
        }
        console.power_off()?;
        Ok(())
    }
}
