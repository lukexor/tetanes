use super::util;
use crate::console::Console;
use std::{error::Error, path::PathBuf};

const PADDING: f32 = 0.0;

pub trait View {
    fn enter(&mut self);
    fn exit(&mut self);
    fn update(&mut self, timestamp: f64, dt: f64, w: i32, h: i32);
    fn get_title(&self) -> String;
}

pub struct GameView {
    pub title: String,
    pub console: Console,
    pub file_hash: String,
    pub save_path: PathBuf,
    pub sram_path: PathBuf,
    pub texture: u32,
    pub record: bool,
    pub frames: Vec<image::Frame>,
}

impl GameView {
    pub fn new(rom: &PathBuf) -> Result<Self, Box<Error>> {
        let file_hash = util::hash_file(&rom)?;
        let save_path = PathBuf::from(format!(
            "{}/.nes/save/{}.dat",
            util::home_dir(),
            file_hash.clone()
        ));
        let sram_path = PathBuf::from(format!(
            "{}/.nes/sram/{}.dat",
            util::home_dir(),
            file_hash.clone()
        ));
        let texture = util::create_texture();
        Ok(Self {
            title: String::from(rom.to_string_lossy()),
            console: Console::new(rom)?,
            file_hash,
            save_path,
            sram_path,
            texture,
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
        // TODO set audio channel
        // Set key callback:
        //  space: screenshot
        //  R: reset
        //  tab: record
        // Load SRAM
        // let _ = self.console.load_sram(&self.sram_path);
        // unimplemented!();
    }

    fn exit(&mut self) {
        // let _ = self.console.save_sram(&self.sram_path);
        // unimplemented!();
    }

    fn update(&mut self, timestamp: f64, mut dt: f64, w: i32, h: i32) {
        if dt > 1.0 {
            dt = 0.0;
        }

        // TODO Check esc to menu
        // Update controllers

        self.console.step_seconds(dt);
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
        }
        util::set_texture(self.console.buffer());
        // let s1 = w as f32 / 256.0;
        // let s2 = h as f32 / 240.0;
        // let f = 1.0 - PADDING;
        // let (x, y) = if s1 >= s2 {
        //     (f * s2 / s1, f)
        // } else {
        //     (f, f * s1 / s2)
        // };
        // unsafe {
        //     let verts = vec![-x, -y, x, -y, x, y, -x, y];
        //     let mut vbo = 0u32;
        //     gl::GenBuffers(1, &mut vbo);
        //     gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
        //     gl::BufferData(
        //         gl::ARRAY_BUFFER,
        //         std::mem::size_of_val(&verts) as isize,
        //         verts.as_ptr() as *const gl::types::GLvoid,
        //         gl::STATIC_DRAW,
        //     );
        //     gl::DrawArrays(gl::TRIANGLES, 0, 4);
        //     gl::BindTexture(gl::TEXTURE_2D, 0);
        // }
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

    fn update(&mut self, _timestamp: f64, _dt: f64, w: i32, h: i32) {
        unimplemented!();
    }

    fn get_title(&self) -> String {
        "Select a game".to_string()
    }
}
