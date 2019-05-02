mod audio;
mod util;
mod view;
mod window;

use super::console::Console;
use std::{error::Error, path::PathBuf};
use window::Window;

pub struct UI {
    window: Window,
    console: Console,
    roms: Vec<PathBuf>,
    timestamp: f64,
}

impl UI {
    pub fn new(roms: Vec<PathBuf>) -> Result<Self, Box<Error>> {
        if roms.is_empty() {
            return Err("no rom files found or specified".into());
        }
        let window = Window::new()?;
        let mut ui = Self {
            window,
            console: Console::new(),
            roms,
            timestamp: 0.0,
        };
        Ok(ui)
    }

    pub fn run(&mut self) {
        loop {
            self.window.poll_events();
            self.console.step();
            self.window.render(self.console.ppu.render());
            // self.window.enqueue_audio(&mut self.console.apu.samples);
        }
    }

    fn step(&mut self) {
        // let timestamp = self.window.time();
        // let dt = timestamp - self.timestamp;
        // self.timestamp = timestamp;
        // let (w, h) = self.window.get_frame_buffer_size();
        // self.views[self.active_view].update(timestamp, dt, w, h);
    }

    // fn set_title(&mut self, title: &str) {
    //     self.window.set_title(title);
    // }

    // fn set_active_view(&mut self, view: usize) {
    //     // Exit needs to:
    //     //   GameView:
    //     //     - Clear KeyCallback
    //     //     - Clear audio channel
    //     //     - Save SRAM
    //     //   MenuView:
    //     //     - Clear CharCallback
    //     self.views[self.active_view].exit();
    //     self.active_view = view;
    //     // Enter needs to:
    //     //   GameView:
    //     //     - Clear to black color
    //     //     - Set title
    //     //     - Link audio channel
    //     //     - Set KeyCallback
    //     //       : Space - Screenshot
    //     //       : R - Reset
    //     //       : Tab - Record
    //     //     - Load SRAM
    //     //   MenuView:
    //     //     - Clear color to gray
    //     //     - Set title to Select a Game
    //     //     - Set CharCallback??
    //     self.set_title(&self.views[self.active_view].get_title());
    //     self.views[self.active_view].enter();
    //     self.update_time();
    // }

    //     fn update_time(&mut self) {
    //         self.timestamp = self.window.time();
    //     }
}
