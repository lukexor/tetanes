use crate::{draw::Rect, event::PixEvent, pixel::ColorType, PixEngineResult};
use std::path::Path;

#[cfg(not(feature = "wasm-driver"))]
pub(super) mod sdl2;
#[cfg(feature = "wasm-driver")]
pub(super) mod wasm;

#[cfg(feature = "wasm-driver")]
pub(super) fn load_driver(opts: DriverOpts) -> PixEngineResult<wasm::WasmDriver> {
    wasm::WasmDriver::new(opts)
}
#[cfg(not(feature = "wasm-driver"))]
pub(super) fn load_driver(opts: DriverOpts) -> PixEngineResult<sdl2::Sdl2Driver> {
    sdl2::Sdl2Driver::new(opts)
}

// TODO Add DriverErr and DriverResult types
pub(super) trait Driver {
    fn fullscreen(&mut self, window_id: u32, val: bool) -> PixEngineResult<()>;
    fn vsync(&mut self, window_id: u32, val: bool) -> PixEngineResult<()>;
    fn load_icon<P: AsRef<Path>>(&mut self, path: P) -> PixEngineResult<()>;
    fn set_title(&mut self, window_id: u32, title: &str) -> PixEngineResult<()>;
    fn set_size(&mut self, window_id: u32, width: u32, height: u32) -> PixEngineResult<()>;
    fn poll(&mut self) -> PixEngineResult<Vec<PixEvent>>;
    fn clear(&mut self, window_id: u32) -> PixEngineResult<()>;
    fn present(&mut self);
    fn create_texture(
        &mut self,
        window_id: u32,
        name: &str,
        color_type: ColorType,
        src: Rect,
        dst: Rect,
    ) -> PixEngineResult<()>;
    fn copy_texture(&mut self, window_id: u32, name: &str, bytes: &[u8]) -> PixEngineResult<()>;
    fn open_window(&mut self, title: &str, width: u32, height: u32) -> PixEngineResult<u32>;
    fn close_window(&mut self, window_id: u32);
    fn enqueue_audio(&mut self, samples: &[f32]);
}

pub(super) struct DriverOpts {
    title: String,
    width: u32,
    height: u32,
    vsync: bool,
}

impl DriverOpts {
    pub(super) fn new(title: &str, width: u32, height: u32, vsync: bool) -> Self {
        Self {
            title: title.to_string(),
            width,
            height,
            vsync,
        }
    }
}
