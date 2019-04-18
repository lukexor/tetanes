use super::*;
use glfw::{Action, Context, Key};
use std::error::Error;
use std::path::PathBuf;

pub fn run(roms: Vec<PathBuf>) -> Result<(), Box<Error>> {
    if roms.is_empty() {
        return Err("no rom files found or specified".into());
    }

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS)?;

    // init fontMask

    glfw.window_hint(glfw::WindowHint::ContextVersionMajor(2));
    glfw.window_hint(glfw::WindowHint::ContextVersionMinor(1));
    let (mut window, events) = glfw
        .create_window(
            WIDTH * SCALE,
            HEIGHT * SCALE,
            TITLE,
            glfw::WindowMode::Windowed,
        )
        .expect("Failed to create GLFW window.");
    window.make_current();
    window.set_key_polling(true);

    gl::load_with(|s| window.get_proc_address(s));
    unsafe {
        gl::Enable(gl::TEXTURE_2D);
    }

    let mut audio = Audio::new();
    // TODO Init audio stream
    let num_roms = roms.len();
    let mut d = Director::new(window, audio, roms);

    if num_roms == 1 {
    } else {
        d.setup_view();
    }

    // main loop
    while !d.window.should_close() {
        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
        let timestamp = glfw.get_time();
        let dt = timestamp - d.timestamp;
        d.timestamp = timestamp;

        // Process view
        let (width, height) = d.window.get_framebuffer_size();
        let sx = 256 + MARGIN * 2;
        let sy = 240 + MARGIN * 2;
        let mut nx = (width - BORDER * 2) / sx;
        let mut ny = (height - BORDER * 2) / sy;
        let ox = (width - nx * sx) / 2 + MARGIN;
        let oy = (height - ny * sy) / 2 + MARGIN;
        if nx < 1 {
            nx = 1;
        }
        if ny < 1 {
            ny = 1;
        }
        d.view.nx = nx;
        d.view.ny = ny;
        // unsafe {
        //     gl::PushMatrix();
        //     gl::Ortho(0, width as f64, height as f64, 0, -1, 1);
        //     gl::BindTexture(gl::TEXTURE_2D, d.view.texture.texture);
        // }
        for j in 0..ny {
            for i in 0..nx {
                let x = (ox + i * sx) as f32;
                let y = (oy + j * sy) as f32;
                let mut index = nx * (j + d.view.scroll) + i;
                if index >= d.view.roms.len() as i32 {
                    continue;
                }
                let rom = &d.view.roms[index as usize];
                index = d.view.texture.lookup_index(rom);
            }
        }

        d.window.swap_buffers();
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            println!("{:?}", event);
            match event {
                glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                    d.window.set_should_close(true)
                }
                _ => {}
            }
        }
    }
    // d.view.clear();
    Ok(())
}
