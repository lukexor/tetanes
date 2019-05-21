pub use sdl2::EventPump;

use crate::console::Console;
use crate::input::{Input, InputResult};
use crate::ui::window::Window;
use crate::Result;
use failure::format_err;
use std::cell::RefCell;
use std::rc::Rc;
use std::{fmt, path::PathBuf};

mod window;

pub struct UI {
    roms: Vec<PathBuf>,
    scale: u32, // 1, 2, or 3
    fullscreen: bool,
    window: Window,
}

impl UI {
    pub fn init(roms: Vec<PathBuf>, scale: u32, fullscreen: bool) -> Result<Self> {
        if roms.is_empty() {
            Err(format_err!("no rom files found or specified"))?;
        }
        Ok(Self {
            roms,
            scale,
            fullscreen,
            window: Window::with_scale(scale)?,
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
        loop {
            console.step_frame();
            self.window.render(&console.render());
            self.window.enqueue_audio(&mut console.audio_samples());

            match console.poll_events() {
                InputResult::Continue => (),
                InputResult::Quit => break,
                InputResult::Reset => console.reset(),
            }
        }
        Ok(())
    }
}
