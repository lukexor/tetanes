pub use sdl2::EventPump;

use crate::console::{Console, Input, InputResult};
use crate::ui::window::Window;
use crate::Result;
use failure::format_err;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::{fmt, path::Path};

mod window;

pub struct UI<P> {
    roms: Vec<P>,
    scale: u32, // 1, 2, or 3
    fullscreen: bool,
    window: Window,
    timestamp: Instant,
}

impl<P: AsRef<Path> + fmt::Debug + Clone> UI<P> {
    pub fn init(roms: Vec<P>, scale: u32, fullscreen: bool) -> Result<Self> {
        if roms.is_empty() {
            Err(format_err!("no rom files found or specified"))?;
        }
        Ok(Self {
            roms,
            scale,
            fullscreen,
            window: Window::with_scale(scale)?,
            timestamp: Instant::now(),
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

    pub fn play_game(&mut self, rom: P) -> Result<()> {
        let event_pump = self.window.event_pump.take().unwrap();
        let input = Rc::new(RefCell::new(Input::init(event_pump)));
        let mut master_clock = Instant::now();
        let mut console = Console::power_on(rom, input.clone())?;
        loop {
            let now = Instant::now();
            let mut dt = now.duration_since(master_clock);
            if dt.as_secs() > 1 {
                // Took too long last time
                dt = Duration::new(0, 0);
            }
            master_clock = now;

            console.step_seconds(dt.as_nanos());
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
