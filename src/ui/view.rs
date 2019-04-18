use super::*;
use image;
use std::path::PathBuf;

pub trait View {
    fn view(&self);
    fn reset(&self, window: &mut glfw::Window);
    fn setup(&self, window: &mut glfw::Window);
}

pub struct GameView {
    pub console: Console,
    pub title: String,
    pub hash: String,
    pub texture: u32,
    pub record: bool,
    pub frames: Vec<image::Frame>,
}

impl View for GameView {
    fn view(&self) {}
    fn reset(&self, window: &mut glfw::Window) {
        window.set_key_polling(false);
        // self.console.set_audio_channel(false);
        // let cartridge = self.console.cartridge;
        // if cartridge.battery != 0 {
        //     write_sram(sram_path(self.hash), cartridge.sram);
        // }
    }
    fn setup(&self, window: &mut glfw::Window) {}
}

pub struct MenuView {
    pub roms: Vec<PathBuf>,
    pub texture: Texture,
    pub nx: i32,
    pub ny: i32,
    pub i: i32,
    pub j: i32,
    pub scroll: i32,
    pub t: f64,
    pub buttons: [bool; 8],
    pub times: [f64; 8],
    pub type_buffer: String,
    pub type_time: f64,
}

impl MenuView {
    pub fn new(roms: Vec<PathBuf>) -> Self {
        MenuView {
            roms,
            texture: Texture::new(),
            nx: 0,
            ny: 0,
            i: 0,
            j: 0,
            scroll: 0,
            t: 0.0,
            buttons: [false; 8],
            times: [0.0; 8],
            type_buffer: String::new(),
            type_time: 0.0,
        }
    }
}

impl View for MenuView {
    fn view(&self) {}
    fn reset(&self, window: &mut glfw::Window) {
        window.set_key_polling(false);
    }
    fn setup(&self, window: &mut glfw::Window) {
        unsafe {
            gl::ClearColor(0.333, 0.333, 0.333, 1.0);
        }
        window.set_title("Select Game");
        window.set_char_polling(true);
    }
}

// let clamp_scroll = |v: &mut MenuView, wrap: bool| {
//     let n = v.paths.len();
//     let mut rows = n / v.nx;
//     if n % v.nx > 0 {
//         rows += 1;
//     }
//     let max_scroll = rows - v.ny;
//     if v.scroll < 0 {
//         if wrap {
//             v.scroll = max_scroll;
//             v.j = v.ny - 1;
//         } else {
//             v.scroll = 0;
//             v.j = 0;
//         }
//     }
//     if v.scroll > max_scroll {
//         if wrap {
//             v.scroll = 0;
//             v.j = 0;
//         } else {
//             v.scroll = max_scroll;
//             v.j = v.ny - 1;
//         }
//     }
// };
