use glfw::{Action, Context, Key};
use std::{error::Error, sync::mpsc};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const SCALE: u32 = 3;
const TITLE: &str = "NES";

pub struct Window {
    window: glfw::Window,
    events: mpsc::Receiver<(f64, glfw::WindowEvent)>,
    glfw: glfw::Glfw,
}

impl Window {
    pub fn new() -> Result<Self, Box<Error>> {
        let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)?;
        glfw.window_hint(glfw::WindowHint::ContextVersionMajor(2));
        glfw.window_hint(glfw::WindowHint::ContextVersionMinor(1));
        let (mut window, events) = glfw
            .create_window(
                WIDTH * SCALE,
                HEIGHT * SCALE,
                TITLE,
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

    pub fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    pub fn should_close(&self) -> bool {
        self.window.should_close()
    }

    pub fn render(&mut self) {
        self.window.swap_buffers();
    }

    pub fn poll_events(&mut self) {
        self.glfw.poll_events();
        for (_, event) in glfw::flush_messages(&self.events) {
            match event {
                glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                    self.window.set_should_close(true)
                }
                _ => {}
            }
        }
    }
}
