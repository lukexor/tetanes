use glfw::{Action, Context, Key, OpenGlProfileHint, WindowHint};
use std::{error::Error, sync::mpsc};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const SCALE: u32 = 3;
const DEFAULT_TITLE: &str = "NES";

pub struct Window {
    window: glfw::Window,
    events: mpsc::Receiver<(f64, glfw::WindowEvent)>,
    glfw: glfw::Glfw,
}

impl Window {
    pub fn new() -> Result<Self, Box<Error>> {
        let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)?;
        glfw.window_hint(WindowHint::ContextVersionMajor(3));
        glfw.window_hint(WindowHint::ContextVersionMinor(3));
        glfw.window_hint(WindowHint::OpenGlProfile(OpenGlProfileHint::Core));
        glfw.window_hint(WindowHint::OpenGlForwardCompat(true));
        glfw.window_hint(WindowHint::Resizable(false));
        let (mut window, events) = glfw
            .create_window(
                WIDTH * SCALE,
                HEIGHT * SCALE,
                DEFAULT_TITLE,
                glfw::WindowMode::Windowed,
            )
            .expect("Failed to create window.");
        window.make_current();
        window.set_key_polling(true);

        gl::load_with(|symbol| window.get_proc_address(symbol));
        unsafe {
            gl::Enable(gl::TEXTURE_2D);
        }

        Ok(Window {
            window,
            events,
            glfw,
        })
    }

    pub fn time(&self) -> f64 {
        self.glfw.get_time()
    }

    pub fn get_frame_buffer_size(&self) -> (i32, i32) {
        self.window.get_framebuffer_size()
    }

    pub fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    pub fn should_close(&self) -> bool {
        self.window.should_close()
    }

    pub fn render(&mut self) {
        self.window.swap_buffers();
    }

    #[allow(clippy::single_match)]
    pub fn poll_events(&mut self) {
        self.glfw.poll_events();
        for (_, event) in glfw::flush_messages(&self.events) {
            match event {
                // TODO Change to pause menu
                glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                    self.window.set_should_close(true)
                }
                _ => {}
            }
        }
    }
}
