use crate::{
    driver::{self, Driver, DriverOpts},
    event::PixEvent,
    pixel,
    state::{State, StateData},
    PixEngineErr, PixEngineResult,
};
use image::DynamicImage;
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Primary PixEngine object that controls Window and StateData
pub struct PixEngine<S>
where
    S: State,
{
    app_name: &'static str,
    state: S,
    should_close: bool,
    icon: DynamicImage,
    data: StateData,
}

impl<S> PixEngine<S>
where
    S: State,
{
    /// Create a new PixEngine instance
    pub fn new(app_name: &'static str, state: S, screen_width: u32, screen_height: u32) -> Self {
        Self {
            app_name,
            state,
            should_close: false,
            icon: DynamicImage::new_rgba8(32, 32),
            data: StateData::new(screen_width, screen_height),
        }
    }
    /// Set a custom window icon
    pub fn set_icon<P: AsRef<Path>>(&mut self, path: P) -> PixEngineResult<()> {
        self.data.driver.load_icon(path)
    }
    /// Toggle fullscreen
    pub fn fullscreen(&mut self, val: bool) {
        self.data.fullscreen(val);
    }
    /// Toggle vsync
    pub fn vsync(&mut self, val: bool) {
        self.data.vsync(val);
    }

    /// Starts the engine loop. Will execute until one of on_create, on_update, or on_destroy
    /// returns false or the Window receives a termination event
    pub fn run(&mut self) -> PixEngineResult<()> {
        if self.data.screen_width() == 0 || self.data.screen_height() == 0 {
            return Err(PixEngineErr::new("invalid screen dimensions"));
        }

        // Create user resources on start up
        let start = self.state.on_start(&mut self.data);
        if start.is_err() {
            return start;
        }

        // Start main loop
        let mut timer = Instant::now();
        let mut frame_timer = Duration::new(0, 0);
        let mut frame_counter = 0;
        let one_second = Duration::new(1, 0);
        let zero_seconds = Duration::new(0, 0);
        while !self.should_close {
            // Extra loop allows on_destroy to prevent closing
            while !self.should_close {
                self.data.events.clear();

                let elapsed = timer.elapsed();
                timer = Instant::now();

                let events: Vec<PixEvent> = self.data.driver.poll();
                for event in events {
                    self.data.events.push(event);
                    match event {
                        PixEvent::Quit | PixEvent::AppTerminating => self.should_close = true,
                        PixEvent::KeyPress(key, pressed, ..) => {
                            self.data.set_new_key_state(key, pressed);
                        }
                        PixEvent::MousePress(button, x, y, pressed) => {
                            // TODO add functionality for mouse click coords
                            self.data.set_new_mouse_state(button, pressed);
                        }
                        PixEvent::MouseMotion(x, y) => self.data.update_mouse(x, y),
                        PixEvent::MouseWheel(delta) => self.data.update_mouse_wheel(delta),
                        PixEvent::Focus(focused) => self.data.set_focused(focused),
                        PixEvent::Background(bg) => {} // TODO
                        PixEvent::Resized => {}
                        PixEvent::None => (), // Do nothing
                    }
                }

                self.data.update_key_states();
                self.data.update_mouse_states();

                // Handle user frame updates
                let update = self.state.on_update(elapsed, &mut self.data);
                if update.is_err() {
                    return update;
                }

                // Display updated frame
                // if self.data.target_dirty {
                //     let pixels = &self.data.get_draw_target().raw_pixels();
                //     self.data.copy_texture("screen", pixels);
                //     self.data.target_dirty = false;
                // }
                self.data.driver.present();

                // Update window title and FPS counter
                frame_timer = frame_timer.checked_add(elapsed).unwrap_or(one_second);
                frame_counter += 1;
                if frame_timer >= one_second {
                    frame_timer = frame_timer.checked_sub(one_second).unwrap_or(zero_seconds);
                    let mut title = format!("{} - FPS: {}", self.app_name, frame_counter);
                    if !self.data.title().is_empty() {
                        title.push_str(&format!(" - {}", self.data.title()));
                    }
                    self.data.driver.set_title(&title)?;
                    frame_counter = 0;
                }
            }

            let on_stop = self.state.on_stop(&mut self.data);
            if on_stop.is_err() {
                return on_stop;
            }
        }

        Ok(())
    }
}
