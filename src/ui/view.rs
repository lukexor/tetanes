use super::util;
use gl::types::*;
use std::ffi::CString;
use std::{error::Error, path::PathBuf};

// const PADDING: f32 = 0.0;

// pub trait View {
//     fn enter(&mut self);
//     fn exit(&mut self);
//     fn update(&mut self, timestamp: f64, dt: f64, w: i32, h: i32);
//     fn get_title(&self) -> String;
// }

// pub struct GameView {
//     pub title: String,
//     pub file_hash: String,
//     pub save_path: PathBuf,
//     pub sram_path: PathBuf,
//     pub texture: u32,
//     pub record: bool,
//     pub frames: Vec<image::Frame>,
// }

// impl GameView {
//     pub fn new(rom: &PathBuf) -> Result<Self, Box<Error>> {
//         let file_hash = util::hash_file(&rom)?;
//         let save_path = PathBuf::from(format!(
//             "{}/.nes/save/{}.dat",
//             util::home_dir(),
//             file_hash.clone()
//         ));
//         let sram_path = PathBuf::from(format!(
//             "{}/.nes/sram/{}.dat",
//             util::home_dir(),
//             file_hash.clone()
//         ));
//         let texture = util::create_texture();
//         Ok(Self {
//             title: String::from(rom.to_string_lossy()),
//             file_hash,
//             save_path,
//             sram_path,
//             texture,
//             record: false,
//             frames: vec![],
//         })
//     }
// }

// impl View for GameView {
//     fn enter(&mut self) {
//         unsafe {
//             gl::ClearColor(0.0, 0.0, 0.0, 1.0);
//         }
//         // TODO set audio channel
//         // Set key callback:
//         //  space: screenshot
//         //  R: reset
//         //  tab: record
//         // Load SRAM
//         // let _ = self.console.load_sram(&self.sram_path);
//         // unimplemented!();
//     }

//     fn exit(&mut self) {
//         // let _ = self.console.save_sram(&self.sram_path);
//         // unimplemented!();
//     }

//     // fn update(&mut self, timestamp: f64, mut dt: f64, w: i32, h: i32) {
//     //     if dt > 1.0 {
//     //         dt = 0.0;
//     //     }

//     //     // TODO Check esc to menu
//     //     // Update controllers

//     //     self.console.step_seconds(dt);
//     //     unsafe {
//     //         gl::BindTexture(gl::TEXTURE_2D, self.texture);
//     //     }
//     //     // util::set_texture(&self.console.buffer());
//     //     let s1 = w as f32 / 256.0;
//     //     let s2 = h as f32 / 240.0;
//     //     let f = 1.0 - PADDING;
//     //     let (x, y) = if s1 >= s2 {
//     //         (f * s2 / s1, f)
//     //     } else {
//     //         (f, f * s1 / s2)
//     //     };
//     //     let verts = vec![-x, -y, x, -y, x, y, -x, y];
//     //     let mut vbo: GLuint = 0;
//     //     let mut vao: GLuint = 0;
//     //     let vert_src = CString::new(
//     //         r"#version 330 core

//     // in vec2 position;

//     // void main()
//     // {
//     // gl_Position = vec4(position, 0.0, 1.0);
//     // }
//     //     ",
//     //     )
//     //     .unwrap();
//     //     let frag_src = CString::new(
//     //         r"#version 330 core

//     // out vec4 outColor;

//     // void main() {
//     // outColor = vec4(1.0, 1.0, 1.0, 1.0);
//     // }
//     //     ",
//     //     )
//     //     .unwrap();
//     //     unsafe {
//     //         gl::GenVertexArrays(1, &mut vao);
//     //         gl::BindVertexArray(vao);

//     //         gl::GenBuffers(1, &mut vbo);
//     //         gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
//     //         gl::BufferData(
//     //             gl::ARRAY_BUFFER,
//     //             (verts.len() * std::mem::size_of::<f32>()) as GLsizeiptr,
//     //             verts.as_ptr() as *const GLvoid,
//     //             gl::STATIC_DRAW,
//     //         );

//     //         let vert_shad = gl::CreateShader(gl::VERTEX_SHADER);
//     //         gl::ShaderSource(vert_shad, 1, &vert_src.as_ptr(), std::ptr::null());
//     //         gl::CompileShader(vert_shad);

//     //         let mut status: GLint = gl::FALSE as GLint;
//     //         gl::GetShaderiv(vert_shad, gl::COMPILE_STATUS, &mut status);
//     //         if (status != gl::TRUE as GLint) {
//     //             eprintln!("Failed to compile");
//     //         }

//     //         let frag_shad = gl::CreateShader(gl::FRAGMENT_SHADER);
//     //         gl::ShaderSource(frag_shad, 1, &frag_src.as_ptr(), std::ptr::null());
//     //         gl::CompileShader(frag_shad);

//     //         gl::GetShaderiv(frag_shad, gl::COMPILE_STATUS, &mut status);
//     //         if (status != gl::TRUE as GLint) {
//     //             eprintln!("Failed to compile");
//     //         }

//     //         let shad_program = gl::CreateProgram();
//     //         gl::AttachShader(shad_program, vert_shad);
//     //         gl::AttachShader(shad_program, frag_shad);
//     //         gl::BindFragDataLocation(shad_program, 0, CString::new("outColor").unwrap().as_ptr());
//     //         gl::LinkProgram(shad_program);
//     //         gl::UseProgram(shad_program);

//     //         let pos_attrib =
//     //             gl::GetAttribLocation(shad_program, CString::new("position").unwrap().as_ptr())
//     //                 as GLuint;
//     //         gl::EnableVertexAttribArray(pos_attrib);
//     //         gl::VertexAttribPointer(pos_attrib, 2, gl::FLOAT, gl::FALSE, 0, std::ptr::null());

//     //         gl::DrawArrays(gl::TRIANGLES, 0, 4);
//     //         gl::BindTexture(gl::TEXTURE_2D, 0);
//     //     }
//     // }

//     fn get_title(&self) -> String {
//         self.title.to_owned()
//     }
// }

// pub struct MenuView {
//     pub roms: Vec<PathBuf>,
// }

// impl MenuView {
//     pub fn new(roms: Vec<PathBuf>) -> Result<Self, Box<Error>> {
//         Ok(Self { roms })
//     }
// }

// impl View for MenuView {
//     fn enter(&mut self) {
//         unimplemented!();
//     }

//     fn exit(&mut self) {
//         unimplemented!();
//     }

//     fn update(&mut self, _timestamp: f64, _dt: f64, w: i32, h: i32) {
//         unimplemented!();
//     }

//     fn get_title(&self) -> String {
//         "Select a game".to_string()
//     }
// }
