use crate::{
    driver::{self, Driver, DriverOpts},
    event::PixEvent,
    pixel::Sprite,
    state::{State, StateData},
    PixEngineErr, Result,
};
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
    fullscreen: bool,
    vsync: bool,
    frame_timer: Duration,
    frame_counter: u32,
    should_close: bool,
    icon: Sprite,
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
            fullscreen: false,
            vsync: false,
            frame_timer: Duration::new(0, 0),
            frame_counter: 0u32,
            should_close: false,
            icon: Sprite::default(),
            data: StateData::new(screen_width as i32, screen_height as i32),
        }
    }
    /// Chain method to enable fullscreen
    pub fn fullscreen(mut self) -> Self {
        self.fullscreen = true;
        self
    }
    /// Chain method to enable vsync
    pub fn vsync(mut self) -> Self {
        self.vsync = true;
        self
    }
    /// Set a custom window icon
    pub fn set_icon<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.icon = Sprite::from_file(path)?;
        Ok(())
    }

    /// Starts the engine loop. Will execute until one of on_create, on_update, or on_destroy
    /// returns false or the Window receives a termination event
    pub fn run(&mut self) -> Result<()> {
        if self.data.screen_width() == 0 || self.data.screen_height() == 0 {
            return Err(PixEngineErr::new("invalid screen dimensions".into()));
        }

        // Initialize backend driver library
        let opts = DriverOpts::new(
            self.data.screen_width() as u32,
            self.data.screen_height() as u32,
            self.fullscreen,
            self.vsync,
            self.icon.clone(),
        );
        let mut driver = driver::load_driver(opts);

        // Create user resources on start up
        if !self.state.on_start(&mut self.data) {
            self.should_close = true;
        }

        // Start main loop
        let mut timer = Instant::now();
        let one_second = Duration::new(1, 0);
        let zero_seconds = Duration::new(0, 0);
        while !self.should_close {
            // Extra loop allows on_destroy to prevent closing
            while !self.should_close {
                let elapsed = timer.elapsed();
                timer = Instant::now();

                let events: Vec<PixEvent> = driver.poll();
                for event in events {
                    match event {
                        PixEvent::Quit | PixEvent::AppTerminating => self.should_close = true,
                        PixEvent::KeyPress(key, pressed) => {
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
                        PixEvent::Resized => {}        // TODO
                        PixEvent::None => (),          // Do nothing
                    }
                }

                self.data.update_key_states();
                self.data.update_mouse_states();

                // Handle user frame updates
                if !self.state.on_update(elapsed, &mut self.data) {
                    self.should_close = true;
                }

                // Clear and update graphics
                driver.clear();
                // if let Some(bytes) = &self.data.raw_bytes() {
                //     driver.update_raw(&bytes);
                // } else {
                driver.update_frame(&self.data.get_draw_target());
                // }

                // Update window title and FPS counter
                self.frame_timer = self.frame_timer.checked_add(elapsed).unwrap_or(one_second);
                self.frame_counter += 1;
                if self.frame_timer >= one_second {
                    self.frame_timer = self
                        .frame_timer
                        .checked_sub(one_second)
                        .unwrap_or(zero_seconds);
                    let mut title = format!("{} - FPS: {}", self.app_name, self.frame_counter);
                    if self.data.title().len() > 0 {
                        title.push_str(&format!(" - {}", self.data.title()));
                    }
                    driver.set_title(&title)?;
                    self.frame_counter = 0;
                }
            }

            if !self.state.on_stop(&mut self.data) {
                self.should_close = false;
            }
        }

        Ok(())
    }
}
