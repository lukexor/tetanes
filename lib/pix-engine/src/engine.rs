use crate::{
    driver::{self, Driver, DriverOpts},
    event::PixEvent,
    state::{State, StateData},
    PixEngineErr, Result,
};
use std::time::{Duration, Instant};

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
    data: StateData,
}

impl<S> PixEngine<S>
where
    S: State,
{
    pub fn new(app_name: &'static str, state: S, screen_width: i32, screen_height: i32) -> Self {
        Self {
            app_name,
            state,
            fullscreen: false,
            vsync: false,
            frame_timer: Duration::new(0, 0),
            frame_counter: 0u32,
            should_close: false,
            data: StateData::new(screen_width, screen_height),
        }
    }
    pub fn fullscreen(mut self, val: bool) -> Self {
        self.fullscreen = val;
        self
    }
    pub fn vsync(mut self, val: bool) -> Self {
        self.vsync = val;
        self
    }

    // Start the engine
    pub fn run(&mut self) -> Result<()> {
        if self.data.screen_width() == 0 || self.data.screen_height() == 0 {
            return Err(PixEngineErr::new("invalid screen dimensions".into()));
        }

        let opts = DriverOpts::new(
            self.data.screen_width() as u32,
            self.data.screen_height() as u32,
            self.fullscreen,
            self.vsync,
        );
        let mut driver = driver::load_driver(opts);

        if !self.state.on_start(&mut self.data) {
            self.should_close = true;
        }

        let mut timer = Instant::now();
        let one_second = Duration::new(1, 0);
        let zero_seconds = Duration::new(0, 0);
        while !self.should_close {
            let elapsed = timer.elapsed();
            timer = Instant::now();

            let events: Vec<PixEvent> = driver.poll();
            for event in events {
                match event {
                    PixEvent::Quit | PixEvent::AppTerminating => {
                        self.should_close = true;
                    }
                    PixEvent::KeyPress(key, pressed) => {
                        self.data.new_key_state[key as usize] = pressed
                    }
                    _ => (),
                }
            }

            self.data.update_key_state();

            if !self.state.on_update(elapsed, &mut self.data) {
                self.should_close = true;
            }

            driver.clear();
            driver.update_frame(&self.data.get_draw_target());

            self.frame_timer = self.frame_timer.checked_add(elapsed).unwrap_or(one_second);
            self.frame_counter += 1;
            if self.frame_timer >= one_second {
                self.frame_timer = self
                    .frame_timer
                    .checked_sub(one_second)
                    .unwrap_or(zero_seconds);
                driver.set_title(&format!("{} - FPS: {}", self.app_name, self.frame_counter))?;
                self.frame_counter = 0;
            }
        }

        if !self.state.on_stop(&mut self.data) {
            self.should_close = false;
        }

        Ok(())
    }
}
