use crate::{
    event::{Input, Key, Mouse},
    pixel::{self, AlphaMode, Pixel, Sprite},
    Result,
};
use std::{path::Path, time::Duration};

mod draw;
pub mod transform;

static DEFAULT_DRAW_COLOR: Pixel = pixel::WHITE;

pub trait State {
    fn on_start(&mut self, _data: &mut StateData) -> bool {
        false
    }
    fn on_stop(&mut self, _data: &mut StateData) -> bool {
        false
    }
    fn on_update(&mut self, _elapsed: Duration, _data: &mut StateData) -> bool {
        true
    }
}

/// Manages all engine state including graphics and inputs
// TODO add stroke for line drawing
pub struct StateData {
    title: String,
    screen_width: i32,
    screen_height: i32,
    default_draw_target: Sprite,
    draw_target: Option<Sprite>,
    // raw_bytes: Option<Vec<u8>>,
    draw_color: Pixel,
    draw_scale: i32,
    font_scale: i32,
    alpha_mode: AlphaMode,
    blend_factor: f32,
    mouse_x: i32,
    mouse_y: i32,
    mouse_wheel_delta: i32,
    font: Sprite,
    has_input_focus: bool,
    has_mouse_focus: bool,
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
    /// Screen Width
    pub fn screen_width(&self) -> i32 {
        self.screen_width
    }
    /// Screen Height
    pub fn screen_height(&self) -> i32 {
        self.screen_height
    }
    /// Change screen dimensions. Clears draw target and resets draw color.
    pub fn set_screen_size(&mut self, w: i32, h: i32) {
        self.screen_width = w;
        self.screen_height = h;
        self.default_draw_target = Sprite::with_size(w, h);
        self.draw_target = None;
        self.set_draw_color(pixel::BLACK);
        self.fill(pixel::BLACK);
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
    pub fn get_mouse_x(&self) -> i32 {
        self.mouse_x
    }
    /// Get Mouse Y-coord in screen space
    pub fn get_mouse_y(&self) -> i32 {
        self.mouse_y
    }
    /// Get Mouse wheel data
    pub fn get_mouse_wheel(&self) -> i32 {
        self.mouse_wheel_delta
    }

    /// Utility functions ======================================================

    /// Collision detection for basic circle shapes
    /// Detects whether point (x, y) lies inside circle of radius r located at (cx, cy)
    pub fn is_inside_circle(&self, cx: f32, cy: f32, r: f32, x: f32, y: f32) -> bool {
        ((x - cx).powf(2.0) + (y - cy).powf(2.0)).sqrt() < r
    }
}

impl StateData {
    pub(super) fn new(screen_width: i32, screen_height: i32) -> Self {
        let font = StateData::construct_font();
        let mut state_data = Self {
            title: String::new(),
            screen_width,
            screen_height,
            default_draw_target: Sprite::with_size(screen_width, screen_height),
            draw_target: None,
            // raw_bytes: None,
            draw_color: DEFAULT_DRAW_COLOR,
            draw_scale: 1i32,
            font_scale: 2i32,
            alpha_mode: AlphaMode::Normal,
            blend_factor: 1.0,
            mouse_x: 0i32,
            mouse_y: 0i32,
            mouse_wheel_delta: 0i32,
            has_input_focus: true,
            has_mouse_focus: true,
            font,
            old_key_state: [false; 256],
            new_key_state: [false; 256],
            key_state: [Input::new(); 256],
            old_mouse_state: [false; 5],
            new_mouse_state: [false; 5],
            mouse_state: [Input::new(); 5],
            coord_wrapping: false,
        };
        state_data.fill(pixel::BLACK);
        state_data
    }
    pub(super) fn set_focused(&mut self, val: bool) {
        self.has_input_focus = val;
    }
    pub(super) fn update_mouse(&mut self, x: i32, y: i32) {
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
