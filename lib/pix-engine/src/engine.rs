use crate::{
    driver::Driver,
    event::PixEvent,
    state::{State, StateData},
    PixEngineErr, PixEngineResult,
};
use std::collections::VecDeque;

const FPS_SAMPLE_SIZE: usize = 30;

/// Primary PixEngine object that controls Window and StateData
pub struct PixEngine<S>
where
    S: State,
{
    app_name: &'static str,
    state: S,
    should_close: bool,
    debug: bool,
    data: StateData,
}

impl<S> PixEngine<S>
where
    S: State,
{
    /// Create a new PixEngine instance
    pub fn new(
        app_name: &'static str,
        state: S,
        screen_width: u32,
        screen_height: u32,
        vsync: bool,
    ) -> PixEngineResult<Self> {
        Ok(Self {
            app_name,
            state,
            should_close: false,
            debug: false,
            data: StateData::new(app_name, screen_width, screen_height, vsync)?,
        })
    }
    /// Set a custom window icon
    pub fn set_icon(&mut self, path: &str) -> PixEngineResult<()> {
        self.data.driver.load_icon(path)
    }
    /// Toggle fullscreen
    pub fn fullscreen(&mut self, val: bool) -> PixEngineResult<()> {
        self.data.fullscreen(val)
    }
    /// Toggle vsync
    pub fn vsync(&mut self, val: bool) -> PixEngineResult<()> {
        self.data.vsync(val)
    }

    /// Starts the engine loop. Will execute until one of on_create, on_update, or on_destroy
    /// returns false or the Window receives a termination event
    pub fn run(&mut self) -> PixEngineResult<()> {
        use std::time::{Duration, Instant};

        if self.data.screen_width() == 0 || self.data.screen_height() == 0 {
            return Err(PixEngineErr::new("invalid screen dimensions"));
        }

        // Create user resources on start up
        match self.state.on_start(&mut self.data) {
            Ok(false) => return Ok(()),
            Err(e) => return Err(e),
            _ => (), // continue on
        }

        // Average FPS if debug enabled
        let mut fps_samples = VecDeque::new();

        // Start main loop
        let main_screen = format!("screen{}", self.data.main_window()); // TODO abstract this out
        let mut timer = Instant::now();
        let mut frame_timer = Duration::new(0, 0);
        let mut frame_counter = 0;
        let one_second = Duration::new(1, 0);
        let zero_seconds = Duration::new(0, 0);
        while !self.should_close {
            // Extra loop allows on_destroy to prevent closing
            while !self.should_close {
                let elapsed = timer.elapsed();
                timer = Instant::now();

                let events: Vec<PixEvent> = self.data.driver.poll()?;
                self.data.events.clear();
                for event in events {
                    self.data.events.push(event);
                    match event {
                        PixEvent::Quit | PixEvent::AppTerminating => self.should_close = true,
                        PixEvent::WinClose(window_id) => {
                            if window_id == self.data.main_window() {
                                self.should_close = true;
                            } else {
                                self.data.driver.close_window(window_id);
                            }
                        }
                        PixEvent::KeyPress(key, pressed, ..) => {
                            self.data.set_new_key_state(key, pressed);
                        }
                        PixEvent::MousePress(button, .., pressed) => {
                            // TODO add functionality for mouse click coords
                            self.data.set_new_mouse_state(button, pressed);
                        }
                        PixEvent::MouseMotion(x, y) => self.data.update_mouse(x, y),
                        PixEvent::MouseWheel(delta) => self.data.update_mouse_wheel(delta),
                        PixEvent::Focus(_, focused) => self.data.set_focused(focused),
                        _ => (), // Skip anything else
                    }
                }

                self.data.update_key_states();
                self.data.update_mouse_states();

                // Handle user frame updates
                match self.state.on_update(elapsed.as_secs_f32(), &mut self.data) {
                    Ok(false) => self.should_close = true,
                    Err(e) => return Err(e),
                    _ => (), // continue on
                }

                // Display updated frame
                if self.data.default_target_dirty {
                    self.data
                        .copy_draw_target(self.data.main_window(), &main_screen)?;
                }
                self.data.driver.present();

                // Update window title and FPS counter
                frame_timer = frame_timer.checked_add(elapsed).unwrap_or(one_second);
                frame_counter += 1;
                if frame_timer >= one_second {
                    frame_timer = frame_timer.checked_sub(one_second).unwrap_or(zero_seconds);
                    if self.debug {
                        fps_samples.push_back(frame_counter);
                        if fps_samples.len() > FPS_SAMPLE_SIZE {
                            let _ = fps_samples.pop_front();
                        }
                    }
                    let mut title = format!("{} - FPS: {}", self.app_name, frame_counter);
                    if !self.data.title().is_empty() {
                        title.push_str(&format!(" - {}", self.data.title()));
                    }
                    self.data
                        .driver
                        .set_title(self.data.main_window(), &title)?;
                    frame_counter = 0;
                }
            }

            match self.state.on_stop(&mut self.data) {
                Ok(false) => self.should_close = false,
                Err(e) => return Err(e),
                _ => (), // continue on
            }
        }

        if self.debug {
            let fps_avg = if !fps_samples.is_empty() {
                fps_samples.iter().sum::<u32>() as f32 / fps_samples.len() as f32
            } else {
                0.0
            };
            println!("Average FPS: {}", fps_avg);
        }

        Ok(())
    }
}
