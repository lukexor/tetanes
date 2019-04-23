use super::{
    audio::Audio,
    util::{hash_file, home_dir},
};
use crate::core::console::Console;
use std::{error::Error, path::PathBuf};

pub trait View {
    fn enter(&mut self);
    fn exit(&mut self);
    fn update(&mut self, timestamp: f64, dt: f64);
    fn get_title(&self) -> String;
}

pub struct GameView {
    pub title: String,
    pub console: Console,
    pub file_hash: String,
    pub save_path: PathBuf,
    pub sram_path: PathBuf,
    pub audio: Audio,
    pub record: bool,
    pub frames: Vec<image::Frame>,
}

impl GameView {
    pub fn new(rom: &PathBuf) -> Result<Self, Box<Error>> {
        let audio = Audio::new()?;
        let file_hash = hash_file(&rom)?;
        let save_path = PathBuf::from(format!(
            "{}/.nes/save/{}.dat",
            home_dir(),
            file_hash.clone()
        ));
        let sram_path = PathBuf::from(format!(
            "{}/.nes/sram/{}.dat",
            home_dir(),
            file_hash.clone()
        ));
        Ok(Self {
            title: String::from(rom.to_string_lossy()),
            console: Console::new(rom)?,
            file_hash,
            save_path,
            sram_path,
            audio,
            record: false,
            frames: vec![],
        })
    }
}

impl View for GameView {
    fn enter(&mut self) {
        unsafe {
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
        }
        let _ = self.console.load_sram(&self.sram_path);
        unimplemented!();
    }

    fn exit(&mut self) {
        let _ = self.console.save_sram(&self.sram_path);
        unimplemented!();
    }

    fn update(&mut self, _timestamp: f64, _dt: f64) {
        unimplemented!();
    }

    fn get_title(&self) -> String {
        self.title.to_owned()
    }
}

pub struct MenuView {
    pub roms: Vec<PathBuf>,
}

impl MenuView {
    pub fn new(roms: Vec<PathBuf>) -> Result<Self, Box<Error>> {
        Ok(Self { roms })
    }
}

impl View for MenuView {
    fn enter(&mut self) {
        unimplemented!();
    }

    fn exit(&mut self) {
        unimplemented!();
    }

    fn update(&mut self, _timestamp: f64, _dt: f64) {
        unimplemented!();
    }

    fn get_title(&self) -> String {
        "Select a game".to_string()
    }
}
