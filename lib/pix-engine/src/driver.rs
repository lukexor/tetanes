use crate::{draw::Rect, event::PixEvent, pixel::ColorType, PixEngineResult};
use image::{DynamicImage, Rgba};
use std::collections::HashMap;
use std::{path::Path, rc::Rc};

#[cfg(not(feature = "wasm-driver"))]
pub(super) mod sdl2;
#[cfg(feature = "wasm-driver")]
pub(super) mod wasm;

#[cfg(feature = "wasm-driver")]
pub(super) fn load_driver(opts: DriverOpts) -> wasm::WasmDriver {
    wasm::WasmDriver::new(opts)
}
#[cfg(not(feature = "wasm-driver"))]
pub(super) fn load_driver(opts: DriverOpts) -> sdl2::Sdl2Driver {
    sdl2::Sdl2Driver::new(opts)
}

// TODO Add DriverErr and DriverResult types
pub(super) trait Driver {
    fn fullscreen(&mut self, val: bool);
    fn vsync(&mut self, val: bool);
    fn load_icon<P: AsRef<Path>>(&mut self, path: P) -> PixEngineResult<()>;
    fn set_title(&mut self, title: &str) -> PixEngineResult<()>;
    fn set_size(&mut self, width: u32, height: u32);
    fn poll(&mut self) -> Vec<PixEvent>;
    fn clear(&mut self);
    fn present(&mut self);
    fn create_texture(&mut self, name: &'static str, color_type: ColorType, src: Rect, dst: Rect);
    fn update_texture(&mut self, name: &'static str, src: Rect, dst: Rect);
    fn copy_texture(&mut self, name: &str, bytes: &[u8]);
    fn copy_texture_dst(&mut self, name: &str, dst: Rect, bytes: &[u8]);
    fn draw_point(&mut self, x: u32, y: u32, p: Rgba<u8>);
    fn enqueue_audio(&mut self, samples: &[f32]);
}

pub(super) struct DriverOpts {
    title: String,
    width: u32,
    height: u32,
}

impl DriverOpts {
    pub(super) fn new(title: &str, width: u32, height: u32) -> Self {
        Self {
            title: title.to_string(),
            width,
            height,
        }
    }
}
