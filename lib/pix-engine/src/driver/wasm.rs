use crate::{
    driver::{Driver, DriverOpts},
    event::PixEvent,
    pixel::Sprite,
    Result,
};

pub struct WasmDriver {}

impl WasmDriver {
    pub fn new(opts: DriverOpts) -> Self {
        Self {}
    }
}

impl Driver for WasmDriver {}
