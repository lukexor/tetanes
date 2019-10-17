use crate::{
    driver::{self, Driver, DriverOpts},
    event::{Input, Key, Mouse, PixEvent},
    pixel::{self, Pixel},
    sprite::Sprite,
    PixEngineErr, PixEngineResult,
};
use std::time::Duration;

pub mod draw;
pub mod transform;

pub trait State {
    fn on_start(&mut self, _data: &mut StateData) -> PixEngineResult<()> {
        Ok(())
    }
    fn on_stop(&mut self, _data: &mut StateData) -> PixEngineResult<()> {
        Ok(())
    }
    fn on_update(&mut self, _elapsed: Duration, _data: &mut StateData) -> PixEngineResult<()> {
        Err(PixEngineErr::new("on_update must be implemented"))
    }
}

/// Pixel blending mode
///   Normal: Ignores alpha channel blending
///   Mask: Only displays pixels if alpha == 255
///   Blend: Blends together alpha channels
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum AlphaMode {
    Normal, // Ignore alpha channel
    Mask,   // Only blend alpha if less than 255
    Blend,  // Always blend alpha
}

/// Manages all engine state including graphics and inputs
// TODO add stroke for line drawing
pub struct StateData {
    pub(super) default_target_dirty: bool,
    #[cfg(all(feature = "sdl2-driver", not(feature = "wasm-driver")))]
    pub(super) driver: driver::sdl2::Sdl2Driver,
    #[cfg(all(feature = "wasm-driver", not(feature = "sdl2-driver")))]
    pub(super) driver: driver::wasm::WasmDriver,
    pub(super) events: Vec<PixEvent>,
    title: String,
    screen_width: u32,
    screen_height: u32,
    default_draw_target: Sprite,
    draw_target: Option<*mut Sprite>,
    default_draw_color: Pixel,
    draw_color: Pixel,
    draw_scale: u32,
    alpha_mode: AlphaMode,
    blend_factor: f32,
    mouse_x: u32,
    mouse_y: u32,
    mouse_wheel_delta: i32,
    font: Sprite,
    has_input_focus: bool,
    old_key_state: [bool; 256],
    new_key_state: [bool; 256],
    key_state: [Input; 256],
    old_mouse_state: [bool; 5],
    new_mouse_state: [bool; 5],
    mouse_state: [Input; 5],
    coord_wrapping: bool,
}

impl StateData {
    /// Engine attributes ======================================================

    /// Custom title to append in the window
    pub fn title(&self) -> &str {
        &self.title
    }
    /// Set a custom title to append
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }
    /// Toggle fullscreen
    pub fn fullscreen(&mut self, val: bool) -> PixEngineResult<()> {
        self.driver.fullscreen(1, val)
    }
    /// Toggle vsync
    pub fn vsync(&mut self, val: bool) -> PixEngineResult<()> {
        self.driver.vsync(1, val)
    }
    /// Screen Width
    pub fn screen_width(&self) -> u32 {
        self.screen_width
    }
    /// Screen Height
    pub fn screen_height(&self) -> u32 {
        self.screen_height
    }
    /// Change screen dimensions.
    pub fn set_screen_size(&mut self, width: u32, height: u32) -> PixEngineResult<()> {
        let mut new_draw_target = Sprite::new(width, height);
        for x in 0..std::cmp::min(width, self.screen_width) {
            for y in 0..std::cmp::min(width, self.screen_height) {
                let p = self.default_draw_target.get_pixel(x, y);
                new_draw_target.put_pixel(x, y, p);
            }
        }
        self.default_draw_target = new_draw_target;
        self.screen_width = width;
        self.screen_height = height;
        self.driver.set_size(1, width, height)
    }
    /// Whether window has focus
    pub fn is_focused(&self) -> bool {
        self.has_input_focus
    }
    /// Get the state of a specific keyboard button
    pub fn get_key(&self, key: Key) -> Input {
        self.key_state[key as usize]
    }
    /// Get the state of a specific mouse buton
    pub fn get_mouse(&self, button: Mouse) -> Input {
        self.mouse_state[button as usize]
    }
    /// Get Mouse X-coord in screen space
    pub fn get_mouse_x(&self) -> u32 {
        self.mouse_x
    }
    /// Get Mouse Y-coord in screen space
    pub fn get_mouse_y(&self) -> u32 {
        self.mouse_y
    }
    /// Get Mouse wheel data
    pub fn get_mouse_wheel(&self) -> i32 {
        self.mouse_wheel_delta
    }
    pub fn poll(&mut self) -> Vec<PixEvent> {
        self.events.drain(..).collect()
    }

    /// Utility functions ======================================================

    /// Collision detection for basic circle shapes
    /// Detects whether point (x, y) lies inside circle of radius r located at (cx, cy)
    pub fn is_inside_circle(&self, cx: f32, cy: f32, r: f32, x: f32, y: f32) -> bool {
        ((x - cx).powf(2.0) + (y - cy).powf(2.0)).sqrt() < r
    }
}

impl StateData {
    pub(super) fn new(
        app_name: &str,
        screen_width: u32,
        screen_height: u32,
        vsync: bool,
    ) -> PixEngineResult<Self> {
        let font = StateData::construct_font();
        // Initialize backend driver library
        let opts = DriverOpts::new(app_name, screen_width, screen_height, vsync);
        let state_data = Self {
            default_target_dirty: false,
            driver: driver::load_driver(opts)?,
            events: Vec::new(),
            title: String::new(),
            screen_width,
            screen_height,
            default_draw_target: Sprite::new(screen_width, screen_height),
            draw_target: None,
            default_draw_color: pixel::WHITE,
            draw_color: pixel::WHITE,
            draw_scale: 1,
            alpha_mode: AlphaMode::Normal,
            blend_factor: 1.0,
            mouse_x: 0,
            mouse_y: 0,
            mouse_wheel_delta: 0,
            has_input_focus: true,
            font,
            old_key_state: [false; 256],
            new_key_state: [false; 256],
            key_state: [Input::new(); 256],
            old_mouse_state: [false; 5],
            new_mouse_state: [false; 5],
            mouse_state: [Input::new(); 5],
            coord_wrapping: false,
        };
        Ok(state_data)
    }
    pub(super) fn set_focused(&mut self, val: bool) {
        self.has_input_focus = val;
    }
    pub(super) fn update_mouse(&mut self, x: u32, y: u32) {
        self.mouse_x = x;
        self.mouse_y = y;
    }
    pub(super) fn update_mouse_wheel(&mut self, delta: i32) {
        self.mouse_wheel_delta += delta;
    }
    pub(super) fn set_new_mouse_state(&mut self, button: Mouse, pressed: bool) {
        self.new_mouse_state[button as usize] = pressed;
    }
    pub(super) fn update_mouse_states(&mut self) {
        for i in 0..self.mouse_state.len() {
            self.mouse_state[i].pressed = false;
            self.mouse_state[i].released = false;
            if self.new_mouse_state[i] != self.old_mouse_state[i] {
                if self.new_mouse_state[i] {
                    self.mouse_state[i].pressed = !self.mouse_state[i].held;
                    self.mouse_state[i].held = true;
                } else {
                    self.mouse_state[i].released = true;
                    self.mouse_state[i].held = false;
                }
            }
            self.old_mouse_state[i] = self.new_mouse_state[i];
        }
    }
    pub(super) fn set_new_key_state(&mut self, key: Key, pressed: bool) {
        self.new_key_state[key as usize] = pressed;
    }
    pub(super) fn update_key_states(&mut self) {
        for i in 0..self.key_state.len() {
            self.key_state[i].pressed = false;
            self.key_state[i].released = false;
            if self.new_key_state[i] != self.old_key_state[i] {
                if self.new_key_state[i] {
                    self.key_state[i].pressed = !self.key_state[i].held;
                    self.key_state[i].held = true;
                } else {
                    self.key_state[i].released = true;
                    self.key_state[i].held = false;
                }
            }
            self.old_key_state[i] = self.new_key_state[i];
        }
    }
}
