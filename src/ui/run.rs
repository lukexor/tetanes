use super::{
    view::{GameView, MenuView, View},
    window::Window,
};
use std::{error::Error, path::PathBuf};

pub struct UI {
    window: Window,
    active_view: usize,
    views: Vec<Box<View>>,
    timestamp: f64,
}

impl UI {
    pub fn run(roms: Vec<PathBuf>) -> Result<(), Box<Error>> {
        if roms.is_empty() {
            return Err("no rom files found or specified".into());
        }
        let mut ui = UI::new(roms)?;
        ui.start()
    }

    pub fn new(roms: Vec<PathBuf>) -> Result<Self, Box<Error>> {
        let window = Window::new()?;
        let mut views: Vec<Box<View>> = vec![Box::new(MenuView::new(roms.clone())?)];
        if roms.len() == 1 {
            views.push(Box::new(GameView::new(&roms[0])?));
        }
        let mut ui = Self {
            window,
            active_view: views.len() - 1,
            views,
            timestamp: 0.0,
        };
        ui.set_active_view(ui.active_view);
        Ok(ui)
    }

    pub fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    pub fn set_active_view(&mut self, view: usize) {
        // Exit needs to:
        //   GameView:
        //     - Clear KeyCallback
        //     - Clear audio channel
        //     - Save SRAM
        //   MenuView:
        //     - Clear CharCallback
        self.views[self.active_view].exit();
        self.active_view = view;
        // Enter needs to:
        //   GameView:
        //     - Clear to black color
        //     - Set title
        //     - Link audio channel
        //     - Set KeyCallback
        //       : Space - Screenshot
        //       : R - Reset
        //       : Tab - Record
        //     - Load SRAM
        //   MenuView:
        //     - Clear color to gray
        //     - Set title to Select a Game
        //     - Set CharCallback??
        self.set_title(&self.views[self.active_view].get_title());
        self.views[self.active_view].enter();
        self.update_time();
    }

    pub fn update_time(&mut self) {
        self.timestamp = self.window.time();
    }

    pub fn start(&mut self) -> Result<(), Box<Error>> {
        while !self.window.should_close() {
            self.step();
            self.window.render();
            self.window.poll_events();
        }
        Ok(())
    }

    pub fn step(&mut self) {
        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
        let timestamp = self.window.time();
        let dt = timestamp - self.timestamp;
        self.timestamp = timestamp;
        self.views[self.active_view].update(timestamp, dt);
    }
}
