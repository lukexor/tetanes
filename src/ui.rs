use crate::console::input::Input;
use crate::console::Console;
use crate::ui::window::Window;
use crate::Result;
use failure::format_err;
use std::{fmt, path::Path};

mod window;

pub struct UI<P> {
    console: Console,
    roms: Vec<P>,
    scale: u32, // 1, 2, or 3
    fullscreen: bool,
}

impl<P: AsRef<Path> + fmt::Debug> UI<P> {
    pub fn init(roms: Vec<P>, scale: u32, fullscreen: bool) -> Result<Self> {
        if roms.is_empty() {
            Err(format_err!("no rom files found or specified"))?;
        }
        Ok(Self {
            console: Console::new(),
            roms,
            scale,
            fullscreen,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let (mut window, event_pump) = Window::with_scale(self.scale)?;
        self.console.load_input(event_pump);
        if self.roms.len() == 1 {
            self.console.load_cartridge(&self.roms[0].as_ref())?;
        }

        // TODO
        // audio::open(&sdl);

        loop {
            let ppu_result = self.console.step();
            if ppu_result.new_frame {
                window.render(&self.console.render());
                // Play audio
            }
            self.console.poll_events();
        }

        // audio::close();
        Ok(())
    }
}
