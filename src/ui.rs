use crate::console::Console;
use crate::ui::window::Window;
use crate::Result;
use failure::format_err;
use std::{fmt, path::Path};

mod window;

pub struct UI<P> {
    console: Option<Console>,
    roms: Vec<P>,
    scale: u32, // 1, 2, or 3
    fullscreen: bool,
}

impl<P: AsRef<Path> + fmt::Debug> UI<P> {
    pub fn init(scale: u32, fullscreen: bool) -> Self {
        Self {
            console: None,
            roms: vec![],
            scale,
            fullscreen,
        }
    }

    pub fn run(&mut self, roms: Vec<P>) -> Result<()> {
        if roms.is_empty() {
            Err(format_err!("no rom files found or specified"))?;
        }

        self.roms = roms;
        if self.roms.len() == 1 {
            let mut console = Console::new();
            console.load_cartridge(&self.roms[0].as_ref())?;
            self.console = Some(console);
        }

        let mut window = Window::with_scale(self.scale)?;
        // TODO
        // audio::open(&sdl);
        // input::new(sdl);

        eprintln!("UI running: {:?}", self.roms);
        loop {
            if let Some(console) = &mut self.console {
                let ppu_result = console.step();
                if ppu_result.new_frame {
                    window.render(&console.render());
                    // Render frame
                    // Play audio
                    // Poll events
                }
            }
            window.poll_events();
            // let sleep = std::time::Duration::from_millis(1000);
            // std::thread::sleep(sleep);
        }

        // audio::close();
        Ok(())
    }
}
