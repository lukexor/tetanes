use super::{
    audio::Audio,
    util,
    view::{GameView, MenuView, View},
    window::Window,
};
use std::{error::Error, path::PathBuf};

const VERT_SRC: &str = r"
    #version 330 core

    in vec2 position;
    in vec3 color;
    in vec2 texcoord;

    out vec3 Color;
    out vec2 Texcoord;

    void main() {
        Color = color;
        Texcoord = texcoord;
        gl_Position = vec4(position, 0.0, 1.0);
    }
";
const FRAG_SRC: &str = r"
    #version 330 core

    in vec3 Color;
    in vec2 Texcoord;

    out vec4 outColor;
    uniform sampler2D tex;

    void main() {
        outColor = texture(tex, Texcoord) * vec4(Color, 1.0);
    }
";

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
            self.window.render();
            self.window.poll_events();
        }
        Ok(())
    }

    pub fn step(&mut self) {
        let timestamp = self.window.time();
        let dt = timestamp - self.timestamp;
        self.timestamp = timestamp;
        let (w, h) = self.window.get_frame_buffer_size();
        self.views[self.active_view].update(timestamp, dt, w, h);
    }

    fn test_start(&mut self) -> Result<(), Box<Error>> {
        use gl::types::*;
        use std::ffi::CString;

        let float_size = std::mem::size_of::<GLfloat>();
        let uint_size = std::mem::size_of::<GLuint>();

        // Vertex data
        let verts: [GLfloat; 28] = [
            -1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, // top-left
            1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, // top-right
            1.0, -1.0, 0.0, 0.0, 1.0, 1.0, 1.0, // bottom-right
            -1.0, -1.0, 1.0, 1.0, 1.0, 0.0, 1.0, // bottom-left
        ];

        let mut texture: GLuint = 0;
        // Vertex Buffer Object
        let mut vbo: GLuint = 0;
        // Vertex Array Object
        let mut vao: GLuint = 0;
        // Element Array
        let mut ebo: GLuint = 0;
        let elements: [GLuint; 6] = [0, 1, 2, 2, 3, 0];

        // Vertex and Fragment Shader sources
        let vert_src = CString::new(VERT_SRC).unwrap();
        let frag_src = CString::new(FRAG_SRC).unwrap();
        let (shad_program, frag_shad, vert_shad);
        unsafe {
            // Create Vertex Array Object
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);

            // Create Vertex Buffer Object our vertices
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo); // Only one can be active at a time
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (verts.len() * float_size) as GLsizeiptr,
                verts.as_ptr() as *const GLvoid,
                gl::STATIC_DRAW,
            );

            // Element Array
            gl::GenBuffers(1, &mut ebo);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                (elements.len() * uint_size) as GLsizeiptr,
                elements.as_ptr() as *const GLvoid,
                gl::STATIC_DRAW,
            );

            // Create Vertex Shader
            vert_shad = gl::CreateShader(gl::VERTEX_SHADER);
            gl::ShaderSource(vert_shad, 1, &vert_src.as_ptr(), std::ptr::null());
            gl::CompileShader(vert_shad);

            // Check status
            let mut status: GLint = gl::FALSE as GLint;
            gl::GetShaderiv(vert_shad, gl::COMPILE_STATUS, &mut status);
            if (status != gl::TRUE as GLint) {
                let mut length: GLsizei = 0;
                gl::GetShaderiv(vert_shad, gl::INFO_LOG_LENGTH, &mut length);

                let mut buffer: Vec<u8> = Vec::with_capacity(length as usize);
                let buf_ptr = buffer.as_mut_ptr() as *mut GLchar;
                gl::GetShaderInfoLog(vert_shad, 512, std::ptr::null_mut(), buf_ptr);
                buffer.set_len(length as usize);
                match String::from_utf8(buffer) {
                    Ok(log) => eprintln!("{}", log),
                    Err(e) => panic!(),
                }
            }

            // Create Fragment Shader
            frag_shad = gl::CreateShader(gl::FRAGMENT_SHADER);
            gl::ShaderSource(frag_shad, 1, &frag_src.as_ptr(), std::ptr::null());
            gl::CompileShader(frag_shad);

            // Check status
            gl::GetShaderiv(frag_shad, gl::COMPILE_STATUS, &mut status);
            if (status != gl::TRUE as GLint) {
                // let mut buffer = [0u8; 512];
                // let mut length: GLsizei = 0;
                // gl::GetShaderInfoLog(frag_shad, 512, length, buffer);
                // eprintln!("Failed to compile fragment shader: {:?}", buffer);
            }

            // Create Program and attach shaders
            shad_program = gl::CreateProgram();
            gl::AttachShader(shad_program, vert_shad);
            gl::AttachShader(shad_program, frag_shad);
            // Not really needed because only one output
            let out_color = CString::new("outColor").unwrap();
            gl::BindFragDataLocation(shad_program, 0, out_color.as_ptr());
            gl::LinkProgram(shad_program);
            gl::UseProgram(shad_program); // Only one can be active at a time

            // Link Vertex data with Attributes
            let vert_size = (7 * float_size) as GLint;
            let position = CString::new("position").unwrap();
            let pos_attrib = gl::GetAttribLocation(shad_program, position.as_ptr()) as GLuint;
            gl::EnableVertexAttribArray(pos_attrib);
            gl::VertexAttribPointer(
                pos_attrib,
                2,
                gl::FLOAT,
                gl::FALSE,
                vert_size,
                std::ptr::null(),
            );

            let color = CString::new("color").unwrap();
            let color_attrib = gl::GetAttribLocation(shad_program, color.as_ptr()) as GLuint;
            gl::EnableVertexAttribArray(color_attrib);
            gl::VertexAttribPointer(
                color_attrib,
                3,
                gl::FLOAT,
                gl::FALSE,
                vert_size,
                (2 * float_size) as *const GLvoid,
            );

            let texcoord = CString::new("texcoord").unwrap();
            let texcoord_attrib = gl::GetAttribLocation(shad_program, texcoord.as_ptr()) as GLuint;
            gl::EnableVertexAttribArray(texcoord_attrib);
            gl::VertexAttribPointer(
                texcoord_attrib,
                2,
                gl::FLOAT,
                gl::FALSE,
                vert_size,
                (5 * float_size) as *const GLvoid,
            );

            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);

            let image = util::load_image("texture.png")?;
            let pixels = image.as_flat_samples().samples;
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as GLint,
                image.width() as GLint,
                image.height() as GLint,
                0,
                gl::RGBA as GLuint,
                gl::UNSIGNED_BYTE,
                pixels.as_ptr() as *const GLvoid,
            );

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as GLint);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as GLint);
            gl::TexParameteri(
                gl::TEXTURE_2D,
                gl::TEXTURE_WRAP_S,
                gl::CLAMP_TO_EDGE as GLint,
            );
            gl::TexParameteri(
                gl::TEXTURE_2D,
                gl::TEXTURE_WRAP_T,
                gl::CLAMP_TO_EDGE as GLint,
            );
        }

        // Draw
        let mut frame = 0;
        while !self.window.should_close() {
            unsafe {
                gl::ClearColor(0.0, 0.0, 0.0, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                gl::DrawElements(gl::TRIANGLES, 6, gl::UNSIGNED_INT, 0 as *const GLvoid);
            }
            self.window.render();
            self.window.poll_events();
            frame += 1;
        }

        unsafe {
            gl::DeleteTextures(1, &texture);
            gl::DeleteProgram(shad_program);
            gl::DeleteShader(frag_shad);
            gl::DeleteShader(vert_shad);
            gl::DeleteVertexArrays(1, &ebo);
            gl::DeleteBuffers(1, &vbo);
            gl::DeleteVertexArrays(1, &vao);
        }

        Ok(())
    }
}
