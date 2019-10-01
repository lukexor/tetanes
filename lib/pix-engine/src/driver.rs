use crate::{event::PixEvent, pixel::Sprite, Result};
use std::collections::HashMap;
use std::rc::Rc;

mod sdl2;
mod wasm;

pub(super) fn load_driver(opts: DriverOpts) -> impl Driver {
    #[cfg(feature = "wasm-driver")]
    return wasm::WasmDriver::new(opts);
    #[cfg(not(feature = "wasm-driver"))]
    return sdl2::Sdl2Driver::new(opts);
}

pub(super) trait Driver {
    fn setup() -> Result<()> {
        Ok(())
    }
    fn poll(&mut self) -> Vec<PixEvent> {
        Vec::new()
    }
    fn set_title(&mut self, _title: &str) -> Result<()> {
        Ok(())
    }
    fn clear(&mut self) {}
    fn update_frame(&mut self, _sprite: &Sprite) {}
    fn update_raw(&mut self, _bytes: &[u8]) {}
}

pub(super) struct DriverOpts {
    width: u32,
    height: u32,
    fullscreen: bool,
    vsync: bool,
    icon: Sprite,
}

impl DriverOpts {
    pub(super) fn new(
        width: u32,
        height: u32,
        fullscreen: bool,
        vsync: bool,
        icon: Sprite,
    ) -> Self {
        Self {
            width,
            height,
            fullscreen,
            vsync,
            icon,
        }
    }
}
