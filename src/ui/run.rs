use super::{
    audio::Audio,
    view::{GameView, MenuView, View},
    window::Window,
};
use std::{error::Error, path::PathBuf};

pub struct UI {
    window: Window,
    active_view: usize,
    views: Vec<Box<View>>,
    pub audio: Audio,
    timestamp: f64,
}

impl UI {
    pub fn run(roms: Vec<PathBuf>) -> Result<(), Box<Error>> {
        if roms.is_empty() {
            return Err("no rom files found or specified".into());
        }
        let mut ui = UI::new(roms)?;
        ui.test_start()
    }

    pub fn new(roms: Vec<PathBuf>) -> Result<Self, Box<Error>> {
        let window = Window::new()?; // Must load this before views to init OpenGl
        let mut views: Vec<Box<View>> = vec![Box::new(MenuView::new(roms.clone())?)];
        if roms.len() == 1 {
            views.push(Box::new(GameView::new(&roms[0])?));
        }
        let mut ui = Self {
            window,
            active_view: views.len() - 1,
            views,
            audio: Audio::new()?,
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
            self.window.poll_events();
            self.window.render();
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
        let (w, h) = self.window.get_frame_buffer_size();
        self.views[self.active_view].update(timestamp, dt, w, h);
    }

    fn test_teardown(&mut self) {}

    fn test_start(&mut self) -> Result<(), Box<Error>> {
        use gl::types::*;
        use std::ffi::CString;

        // Vertex data
        let verts: [f32; 6] = [0.0, 0.5, 0.5, -0.5, -0.5, -0.5];
        // Vertex Buffer Object
        let mut vbo: GLuint = 0;
        // Vertex Array Object
        let mut vao: GLuint = 0;

        // Vertex and Fragment Shader sources
        let vert_src = CString::new(
            r"#version 330 core

in vec2 position;

void main()
{
    gl_Position = vec4(position, 0.0, 1.0);
}
        ",
        )
        .unwrap();
        let frag_src = CString::new(
            r"#version 330 core

out vec4 outColor;

void main() {
    outColor = vec4(1.0, 1.0, 1.0, 1.0);
}
        ",
        )
        .unwrap();
        unsafe {
            // Create Vertex Array Object
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);

            // Create Vertex Buffer Object our vertices
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo); // Only one can be active at a time
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (verts.len() * std::mem::size_of::<f32>()) as GLsizeiptr,
                verts.as_ptr() as *const GLvoid,
                gl::STATIC_DRAW,
            );

            // Create Vertex Shader
            let vert_shad = gl::CreateShader(gl::VERTEX_SHADER);
            gl::ShaderSource(vert_shad, 1, &vert_src.as_ptr(), std::ptr::null());
            gl::CompileShader(vert_shad);

            // Check status
            let mut status: GLint = gl::FALSE as GLint;
            gl::GetShaderiv(vert_shad, gl::COMPILE_STATUS, &mut status);
            if (status != gl::TRUE as GLint) {
                eprintln!("Failed to compile");
            }

            // Create Fragment Shader
            let frag_shad = gl::CreateShader(gl::FRAGMENT_SHADER);
            gl::ShaderSource(frag_shad, 1, &frag_src.as_ptr(), std::ptr::null());
            gl::CompileShader(frag_shad);

            // Check status
            gl::GetShaderiv(frag_shad, gl::COMPILE_STATUS, &mut status);
            if (status != gl::TRUE as GLint) {
                eprintln!("Failed to compile");
            }

            // Create Program and attach shaders
            let shad_program = gl::CreateProgram();
            gl::AttachShader(shad_program, vert_shad);
            gl::AttachShader(shad_program, frag_shad);
            // Not really needed because only one output
            gl::BindFragDataLocation(shad_program, 0, CString::new("outColor").unwrap().as_ptr());
            gl::LinkProgram(shad_program);
            gl::UseProgram(shad_program); // Only one can be active at a time

            // Link Vertex data with Attributes
            let pos_attrib =
                gl::GetAttribLocation(shad_program, CString::new("position").unwrap().as_ptr())
                    as GLuint;
            gl::EnableVertexAttribArray(pos_attrib);
            gl::VertexAttribPointer(pos_attrib, 2, gl::FLOAT, gl::FALSE, 0, std::ptr::null());

            // Draw
            while !self.window.should_close() {
                self.window.poll_events();
                gl::ClearColor(0.0, 0.0, 0.0, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                gl::DrawArrays(gl::TRIANGLES, 0, 3);
                self.window.render();
            }

            gl::DeleteProgram(shad_program);
            gl::DeleteShader(frag_shad);
            gl::DeleteShader(vert_shad);
            gl::DeleteBuffers(1, &vbo);
            gl::DeleteVertexArrays(1, &vao);
            Ok(())
        }
    }
}
