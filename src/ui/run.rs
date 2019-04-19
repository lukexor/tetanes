use super::{audio::Audio, window::Window};
use crate::core::console::Console;
use image;
use std::{error::Error, path::PathBuf};

const MARGIN: i32 = 10;
const BORDER: i32 = 10;

trait View {
    fn enter(&mut self);
    fn exit(&mut self);
    fn update(&mut self, timestamp: f64, dt: f64);
}

struct GameView {
    console: Console,
    title: String,
    record: bool,
    frames: Vec<image::Frame>,
}

impl GameView {
    pub fn new(rom: &PathBuf) -> Result<Self, Box<Error>> {
        Ok(Self {
            console: Console::new(rom)?,
            title: String::from(rom.to_string_lossy()),
            record: false,
            frames: vec![],
        })
    }
}

impl View for GameView {
    fn enter(&mut self) {}
    fn exit(&mut self) {}
    fn update(&mut self, timestamp: f64, dt: f64) {}
}

struct MenuView {
    roms: Vec<PathBuf>,
}

impl MenuView {
    pub fn new(roms: Vec<PathBuf>) -> Result<Self, Box<Error>> {
        Ok(Self { roms })
    }
}

impl View for MenuView {
    fn enter(&mut self) {}
    fn exit(&mut self) {}
    fn update(&mut self, timestamp: f64, dt: f64) {}
}

pub struct UI {
    window: Window,
    audio: Audio,
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
        let audio = Audio::new()?;
        let window = Window::new()?;
        let num_roms = roms.len();
        let mut views: Vec<Box<View>> = vec![Box::new(MenuView::new(roms.clone())?)];
        if roms.len() == 1 {
            views.push(Box::new(GameView::new(&roms[0])?));
        }
        Ok(Self {
            window,
            audio,
            active_view: views.len() - 1,
            views,
            timestamp: 0.0,
        })
    }

    pub fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    pub fn set_active_view(&mut self, view: usize) {
        self.views[self.active_view].exit();
        self.active_view = view;
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
        self.clear_view();
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

    pub fn clear_view(&mut self) {
        unimplemented!();
    }
}
