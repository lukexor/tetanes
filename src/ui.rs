use crate::console::Console;
use crate::Result;
use failure::format_err;
use std::{fmt, path::Path};

pub struct UI<P> {
    console: Option<Console>,
    roms: Vec<P>,
    scale: u8, // 1, 2, or 3
    fullscreen: bool,
}

impl<P: AsRef<Path> + fmt::Debug> UI<P> {
    pub fn init(scale: u8, fullscreen: bool) -> Self {
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

        if self.roms.len() == 1 {
            let mut console = Console::new();
            console.load_cartridge(&roms[0].as_ref())?;
            self.console = Some(console);
        }
        self.roms = roms;

        // TODO
        // let (window, sdl) = Window::new(scale)
        // audio::open(&sdl)
        // input::new(sdl)

        eprintln!("UI running: {:?}", self.roms);
        loop {
            if let Some(c) = &mut self.console {
                c.step();
            }
            // self.window.poll_events();
            // self.window.render(self.console.ppu.render());
            // self.window.enqueue_audio(&mut self.console.apu.samples);
        }
    }

    //     fn step(&mut self) {
    //         // let timestamp = self.window.time();
    //         // let dt = timestamp - self.timestamp;
    //         // self.timestamp = timestamp;
    //         // let (w, h) = self.window.get_frame_buffer_size();
    //         // self.views[self.active_view].update(timestamp, dt, w, h);
    //     }

    //     // fn set_title(&mut self, title: &str) {
    //     //     self.window.set_title(title);
    //     // }

    //     // fn set_active_view(&mut self, view: usize) {
    //     //     // Exit needs to:
    //     //     //   GameView:
    //     //     //     - Clear KeyCallback
    //     //     //     - Clear audio channel
    //     //     //     - Save SRAM
    //     //     //   MenuView:
    //     //     //     - Clear CharCallback
    //     //     self.views[self.active_view].exit();
    //     //     self.active_view = view;
    //     //     // Enter needs to:
    //     //     //   GameView:
    //     //     //     - Clear to black color
    //     //     //     - Set title
    //     //     //     - Link audio channel
    //     //     //     - Set KeyCallback
    //     //     //       : Space - Screenshot
    //     //     //       : R - Reset
    //     //     //       : Tab - Record
    //     //     //     - Load SRAM
    //     //     //   MenuView:
    //     //     //     - Clear color to gray
    //     //     //     - Set title to Select a Game
    //     //     //     - Set CharCallback??
    //     //     self.set_title(&self.views[self.active_view].get_title());
    //     //     self.views[self.active_view].enter();
    //     //     self.update_time();
    //     // }

    //     //     fn update_time(&mut self) {
    //     //         self.timestamp = self.window.time();
    //     //     }
}
